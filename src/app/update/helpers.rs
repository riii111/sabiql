use unicode_casefold::UnicodeCaseFold;

use crate::domain::DatabaseType;
use crate::domain::connection::SqliteConnectionConfig;
use crate::domain::{QueryResult, QueryValue, TableKind};
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::QueryStatus;
use crate::model::connection::setup::{ConnectionField, ConnectionSetupState};
use crate::policy::write::write_guardrails::{
    TargetSummary, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::policy::write::write_update::build_pk_pairs;
use crate::services::AppServices;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditGuardrailError {
    #[error("No result to edit")]
    NoResult,
    #[error("Only Preview results are editable")]
    NotEditableResult,
    #[error("Preview target table is unknown")]
    UnknownTable,
    #[error("Table metadata not loaded")]
    TableMetadataNotLoaded,
    #[error("Table metadata does not match current preview target")]
    StaleTableMetadata,
    #[error("Preview target is read-only: {0}")]
    ReadOnlyPreviewTarget(&'static str),
    #[error("Editing requires a PRIMARY KEY.")]
    EditingRequiresPrimaryKey,
    #[error("Deletion requires a PRIMARY KEY. This table has no PRIMARY KEY.")]
    DeletionRequiresPrimaryKey,
    #[error("No rows staged for deletion")]
    NoRowsStagedForDeletion,
    #[error("No active connection")]
    NoActiveConnection,
    #[error("Write is unavailable while query is running")]
    WriteUnavailableWhileQueryRunning,
    #[error("Staged row index {0} out of bounds")]
    StagedRowIndexOutOfBounds(usize),
    #[error("Stable key columns are not present in current result")]
    StableKeyColumnsMissing,
    #[error("No active cell edit session")]
    NoActiveCellEditSession,
    #[error("No row selected for edit")]
    NoRowSelectedForEdit,
    #[error("No column selected for edit")]
    NoColumnSelectedForEdit,
    #[error("Row index out of bounds")]
    RowIndexOutOfBounds,
    #[error("Column index out of bounds")]
    ColumnIndexOutOfBounds,
    #[error("Primary key columns are read-only")]
    PrimaryKeyColumnsReadOnly,
    #[error("Read-only column cannot be edited: {0}")]
    ReadOnlyColumn(String),
    #[error("No active row")]
    NoActiveRow,
    #[error("No active cell")]
    NoActiveCell,
    #[error("Cell index out of bounds")]
    CellIndexOutOfBounds,
    #[error("Only text cells can be edited inline")]
    NonTextInlineEdit,
    #[error("SQLite writes require non-NULL primary key values")]
    SqliteNullPrimaryKey,
    #[error("{0}")]
    GuardrailBlocked(String),
}

pub struct BulkDeletePreviewResult {
    pub preview: WritePreview,
    pub target_page: usize,
    pub target_row: Option<usize>,
}

pub fn reject_sqlite_null_pk(
    database_type: DatabaseType,
    pk_pairs: &[(String, QueryValue)],
) -> Result<(), EditGuardrailError> {
    if database_type == DatabaseType::SQLite
        && pk_pairs
            .iter()
            .any(|(_, value)| matches!(value, QueryValue::Null))
    {
        return Err(EditGuardrailError::SqliteNullPrimaryKey);
    }
    Ok(())
}

// Entry checks in navigation and submit-time checks in query should both use this.
// Row/column selection source is intentionally left to each caller:
// navigation uses live selection, query submit uses cell_edit state.
pub fn editable_preview_base(
    state: &AppState,
) -> Result<(&QueryResult, &[String]), EditGuardrailError> {
    let result = state
        .query
        .visible_result()
        .ok_or(EditGuardrailError::NoResult)?;
    if !state.query.can_edit_visible_result() {
        return Err(EditGuardrailError::NotEditableResult);
    }

    if state.query.pagination.schema().is_empty() || state.query.pagination.table().is_empty() {
        return Err(EditGuardrailError::UnknownTable);
    }

    let table_detail = state
        .session
        .table_detail()
        .ok_or(EditGuardrailError::TableMetadataNotLoaded)?;

    if table_detail.schema != state.query.pagination.schema()
        || table_detail.name != state.query.pagination.table()
    {
        return Err(EditGuardrailError::StaleTableMetadata);
    }
    if table_detail.kind_info.kind == TableKind::View {
        return Err(EditGuardrailError::ReadOnlyPreviewTarget("view"));
    }

    let pk_cols = table_detail
        .primary_key
        .as_ref()
        .filter(|cols| !cols.is_empty())
        .map(Vec::as_slice)
        .ok_or(EditGuardrailError::EditingRequiresPrimaryKey)?;

    Ok((result, pk_cols))
}

pub fn ensure_column_writable(
    state: &AppState,
    column_name: &str,
    pk_cols: &[String],
) -> Result<(), EditGuardrailError> {
    if pk_cols.iter().any(|pk| pk == column_name) {
        return Err(EditGuardrailError::PrimaryKeyColumnsReadOnly);
    }

    if let Some(column) = state.session.table_detail().and_then(|table| {
        table
            .columns
            .iter()
            .find(|column| column.name == column_name)
    }) && column.is_read_only()
    {
        let reason = column.read_only_reason().unwrap_or("read-only");
        return Err(EditGuardrailError::ReadOnlyColumn(format!(
            "{column_name} ({reason})"
        )));
    }

    Ok(())
}

pub fn build_bulk_delete_preview(
    state: &AppState,
    services: &AppServices,
) -> Result<BulkDeletePreviewResult, EditGuardrailError> {
    if state.result_interaction.staged_delete_rows().is_empty() {
        return Err(EditGuardrailError::NoRowsStagedForDeletion);
    }
    if state.session.dsn().is_none() {
        return Err(EditGuardrailError::NoActiveConnection);
    }
    if state.query.status() != QueryStatus::Idle {
        return Err(EditGuardrailError::WriteUnavailableWhileQueryRunning);
    }

    let (result, pk_cols) = editable_preview_base(state).map_err(|err| match err {
        EditGuardrailError::EditingRequiresPrimaryKey => {
            EditGuardrailError::DeletionRequiresPrimaryKey
        }
        other => other,
    })?;

    let mut pk_pairs_per_row: Vec<Vec<(String, QueryValue)>> = Vec::new();
    for &row_idx in state.result_interaction.staged_delete_rows() {
        let row = result
            .values()
            .get(row_idx)
            .ok_or(EditGuardrailError::StagedRowIndexOutOfBounds(row_idx))?;
        let pairs = build_pk_pairs(&result.columns, row, pk_cols)
            .ok_or(EditGuardrailError::StableKeyColumnsMissing)?;
        reject_sqlite_null_pk(state.session.active_database_type_or_default(), &pairs)?;
        pk_pairs_per_row.push(pairs);
    }

    let sql = services.sql_dialect.build_bulk_delete_sql(
        state.session.active_database_type_or_default(),
        state.query.pagination.schema(),
        state.query.pagination.table(),
        &pk_pairs_per_row,
    );

    let staged_count = state.result_interaction.staged_delete_rows().len();
    let first_deleted_idx = *state
        .result_interaction
        .staged_delete_rows()
        .iter()
        .next()
        .unwrap();
    let (target_page, target_row) = deletion_refresh_target_bulk(
        result.rows().len(),
        staged_count,
        first_deleted_idx,
        state.query.pagination.current_page(),
    );

    let target = TargetSummary {
        schema: state.query.pagination.schema().to_string(),
        table: state.query.pagination.table().to_string(),
        key_values: pk_pairs_per_row.first().cloned().unwrap_or_default(),
    };
    let guardrail = evaluate_guardrails(true, true, Some(target.clone()));

    Ok(BulkDeletePreviewResult {
        preview: WritePreview {
            operation: WriteOperation::Delete,
            sql,
            target_summary: target,
            diff: vec![],
            guardrail,
        },
        target_page,
        target_row,
    })
}

pub fn deletion_refresh_target_bulk(
    row_count: usize,
    deleted_count: usize,
    first_deleted_idx: usize,
    current_page: usize,
) -> (usize, Option<usize>) {
    let remaining = row_count.saturating_sub(deleted_count);
    if remaining == 0 {
        if current_page > 0 {
            (current_page - 1, Some(usize::MAX))
        } else {
            (0, None)
        }
    } else {
        let target_row = first_deleted_idx.min(remaining - 1);
        (current_page, Some(target_row))
    }
}

pub fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

pub fn find_text_matches(content: &str, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_folded = query.case_fold().collect::<String>();
    let mut matches = Vec::new();
    let mut offset = 0;

    for segment in content.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let (folded, offset_map) = casefold_with_char_offsets(line);
        let mut search_from = 0;
        while let Some(rel_idx) = folded[search_from..].find(&query_folded) {
            let match_idx = search_from + rel_idx;
            matches.push(offset + original_char_offset_for_folded_byte(&offset_map, match_idx));
            search_from =
                folded_byte_offset_after_original_match(&offset_map, match_idx, query_folded.len());
        }
        offset += segment.chars().count();
    }

    matches
}

fn casefold_with_char_offsets(text: &str) -> (String, Vec<(usize, usize)>) {
    let mut folded = String::new();
    let mut offset_map = Vec::new();

    for (original_char_offset, ch) in text.chars().enumerate() {
        for folded_char in ch.case_fold() {
            offset_map.push((folded.len(), original_char_offset));
            folded.push(folded_char);
        }
    }

    offset_map.push((folded.len(), text.chars().count()));
    (folded, offset_map)
}

fn original_char_offset_for_folded_byte(
    offset_map: &[(usize, usize)],
    folded_byte_offset: usize,
) -> usize {
    let idx = offset_map.partition_point(|(byte_offset, _)| *byte_offset <= folded_byte_offset);
    offset_map[idx.saturating_sub(1)].1
}

fn folded_byte_offset_after_original_match(
    offset_map: &[(usize, usize)],
    folded_match_start: usize,
    folded_match_len: usize,
) -> usize {
    let folded_match_end = folded_match_start + folded_match_len;
    let last_matched_original =
        original_char_offset_for_folded_byte(offset_map, folded_match_end.saturating_sub(1));
    offset_map
        .iter()
        .find_map(|(byte_offset, original_offset)| {
            (*byte_offset >= folded_match_end && *original_offset > last_matched_original)
                .then_some(*byte_offset)
        })
        .unwrap_or(folded_match_end)
}

fn text_input_content(state: &ConnectionSetupState, field: ConnectionField) -> &str {
    state
        .input(field)
        .expect("connection field is a text input")
        .content()
}

fn require_non_empty(state: &mut ConnectionSetupState, field: ConnectionField, message: &str) {
    if text_input_content(state, field).trim().is_empty() {
        state.set_validation_error(field, message);
    }
}

#[cfg(test)]
mod text_search_tests {
    use super::find_text_matches;

    #[test]
    fn text_matches_return_first_match_offset_per_line_case_insensitively() {
        let matches = find_text_matches(
            "{\n  \"Theme\": \"dark\",\n  \"theme\": \"light\"\n}",
            "theme",
        );

        assert_eq!(matches, vec![5, 24]);
    }

    #[test]
    fn text_matches_return_empty_for_empty_query() {
        let matches = find_text_matches("{\n  \"theme\": \"dark\"\n}", "");

        assert!(matches.is_empty());
    }

    #[test]
    fn text_matches_map_unicode_casefold_back_to_original_char_offset() {
        let matches = find_text_matches("İx", "x");

        assert_eq!(matches, vec![1]);
    }

    #[test]
    fn text_matches_casefold_german_sharp_s() {
        let matches = find_text_matches("Maße", "MASSE");

        assert_eq!(matches, vec![0]);
    }

    #[test]
    fn text_matches_do_not_duplicate_expanded_casefold_character() {
        let matches = find_text_matches("Maße", "s");

        assert_eq!(matches, vec![2]);
    }

    #[test]
    fn text_matches_casefold_greek_final_sigma() {
        let matches = find_text_matches("ὈΔΥΣΣΕΎΣ", "ὀδυσσεύς");

        assert_eq!(matches, vec![0]);
    }

    #[test]
    fn text_matches_return_all_matches_within_single_line() {
        let matches = find_text_matches("theme theme", "theme");

        assert_eq!(matches, vec![0, 6]);
    }
}

pub fn validate_field(state: &mut ConnectionSetupState, field: ConnectionField) {
    state.clear_validation_error(field);

    if let Some(max_chars) = field.max_chars() {
        let length = state.field_value(field).chars().count();
        if length > max_chars {
            state
                .validation_errors
                .insert(field, format!("Must be {max_chars} characters or less"));
            return;
        }
    }

    match field {
        ConnectionField::SqlitePath => {
            let path = text_input_content(state, ConnectionField::SqlitePath).to_string();
            match SqliteConnectionConfig::new(path) {
                Ok(_) => {}
                Err(error) => state.record_sqlite_config_error(error),
            }
        }
        ConnectionField::Port => {
            let port = text_input_content(state, field).trim();
            if port.is_empty() {
                state.set_validation_error(field, "Required");
            } else {
                match port.parse::<u16>() {
                    Err(_) => {
                        state.set_validation_error(field, "Invalid port");
                    }
                    Ok(0) => {
                        state.set_validation_error(field, "Port must be > 0");
                    }
                    Ok(_) => {}
                }
            }
        }
        ConnectionField::Database => require_non_empty(state, field, "Required"),
        ConnectionField::Name => {
            let name = text_input_content(state, field).trim().to_string();
            if name.is_empty() {
                state.set_validation_error(field, "Name is required");
            }
        }
        ConnectionField::DatabaseType
        | ConnectionField::Host
        | ConnectionField::User
        | ConnectionField::Password
        | ConnectionField::SslMode => {}
    }
}

pub fn validate_all(state: &mut ConnectionSetupState) {
    let active_fields = ConnectionField::fields_for(state.database_type());
    state.retain_validation_errors_for_visible_fields();
    for field in active_fields {
        validate_field(state, *field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Column;
    use std::sync::Arc;

    use crate::domain::connection::ConnectionId;
    use crate::domain::{ColumnAttributes, DatabaseType, QuerySource, Table};
    use rstest::rstest;

    mod validate_field_name {
        use super::*;
        use crate::model::shared::text_input::TextInputState;

        #[test]
        fn empty_name_sets_error() {
            let mut state = ConnectionSetupState::default();

            validate_field(&mut state, ConnectionField::Name);

            assert_eq!(
                state.validation_error(ConnectionField::Name),
                Some("Name is required")
            );
        }

        #[test]
        #[allow(
            clippy::field_reassign_with_default,
            reason = "intentional partial override of Default for clarity"
        )]
        fn whitespace_only_name_sets_error() {
            let mut state = ConnectionSetupState::default();
            *state.input_mut(ConnectionField::Name).unwrap() = TextInputState::new("   ", 3);

            validate_field(&mut state, ConnectionField::Name);

            assert_eq!(
                state.validation_error(ConnectionField::Name),
                Some("Name is required")
            );
        }

        #[rstest]
        #[case("a".repeat(50), false)]
        #[case("a".repeat(51), true)]
        fn name_length_validation(#[case] name: String, #[case] expect_error: bool) {
            let mut state = ConnectionSetupState::default();
            let len = name.chars().count();
            *state.input_mut(ConnectionField::Name).unwrap() = TextInputState::new(name, len);

            validate_field(&mut state, ConnectionField::Name);

            if expect_error {
                assert_eq!(
                    state.validation_error(ConnectionField::Name),
                    Some("Must be 50 characters or less")
                );
            } else {
                assert!(!state.has_validation_error(ConnectionField::Name));
            }
        }

        #[test]
        fn valid_name_clears_previous_error() {
            let mut state = ConnectionSetupState::default();
            validate_field(&mut state, ConnectionField::Name);
            assert!(state.has_validation_error(ConnectionField::Name));

            state
                .input_mut(ConnectionField::Name)
                .unwrap()
                .set_content("Valid Name".to_string());
            validate_field(&mut state, ConnectionField::Name);

            assert!(!state.has_validation_error(ConnectionField::Name));
        }
    }

    mod validate_sqlite_path {
        use super::*;
        use crate::domain::connection::DatabaseType;

        #[test]
        fn empty_path_sets_required_error() {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(DatabaseType::SQLite);
            state
                .input_mut(ConnectionField::SqlitePath)
                .unwrap()
                .set_content("   ".to_string());

            validate_field(&mut state, ConnectionField::SqlitePath);

            assert_eq!(
                state.validation_error(ConnectionField::SqlitePath),
                Some("Required")
            );
        }

        #[test]
        fn unsupported_path_characters_set_error() {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(DatabaseType::SQLite);
            state
                .input_mut(ConnectionField::SqlitePath)
                .unwrap()
                .set_content("/tmp/app\0.db".to_string());

            validate_field(&mut state, ConnectionField::SqlitePath);

            assert_eq!(
                state.validation_error(ConnectionField::SqlitePath),
                Some("Unsupported characters")
            );
        }

        #[test]
        fn in_memory_database_sets_unsupported_format_error() {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(DatabaseType::SQLite);
            state
                .input_mut(ConnectionField::SqlitePath)
                .unwrap()
                .set_content(":memory:".to_string());

            validate_field(&mut state, ConnectionField::SqlitePath);

            assert_eq!(
                state.validation_error(ConnectionField::SqlitePath),
                Some("Use a regular file path (in-memory and URI filenames unsupported)")
            );
        }

        #[test]
        fn uri_filename_sets_unsupported_format_error() {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(DatabaseType::SQLite);
            state
                .input_mut(ConnectionField::SqlitePath)
                .unwrap()
                .set_content("file:/tmp/app.db?mode=ro".to_string());

            validate_field(&mut state, ConnectionField::SqlitePath);

            assert_eq!(
                state.validation_error(ConnectionField::SqlitePath),
                Some("Use a regular file path (in-memory and URI filenames unsupported)")
            );
        }

        #[test]
        fn validate_all_removes_errors_for_hidden_fields() {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(DatabaseType::SQLite);
            state.set_validation_error(ConnectionField::Host, "Required");
            state
                .input_mut(ConnectionField::SqlitePath)
                .unwrap()
                .set_content("/tmp/app.db".to_string());

            validate_all(&mut state);

            assert!(!state.has_validation_error(ConnectionField::Host));
        }
    }

    mod validate_field_max_length {
        use super::*;
        use crate::model::shared::text_input::TextInputState;

        #[rstest]
        #[case(ConnectionField::Host, "a".repeat(255), false)]
        #[case(ConnectionField::Host, "a".repeat(256), true)]
        #[case(ConnectionField::Database, "a".repeat(255), false)]
        #[case(ConnectionField::Database, "a".repeat(256), true)]
        #[case(ConnectionField::User, "a".repeat(255), false)]
        #[case(ConnectionField::User, "a".repeat(256), true)]
        #[case(ConnectionField::Password, "a".repeat(255), false)]
        #[case(ConnectionField::Password, "a".repeat(256), true)]
        fn max_length_validation(
            #[case] field: ConnectionField,
            #[case] value: String,
            #[case] expect_error: bool,
        ) {
            let mut state = ConnectionSetupState::default();
            let len = value.chars().count();
            let input = TextInputState::new(value, len);

            match field {
                ConnectionField::Host => state.host = input,
                ConnectionField::Database => state.database = input,
                ConnectionField::User => state.user = input,
                ConnectionField::Password => state.password = input,
                _ => unreachable!(),
            }

            validate_field(&mut state, field);

            if expect_error {
                assert_eq!(
                    state.validation_errors.get(&field),
                    Some(&"Must be 255 characters or less".to_string())
                );
            } else {
                assert!(!state.validation_errors.contains_key(&field));
            }
        }
    }

    mod delete_refresh_target_bulk {
        use super::*;

        #[test]
        fn all_rows_deleted_first_page_clears_selection() {
            let (page, row) = deletion_refresh_target_bulk(2, 2, 0, 0);
            assert_eq!(page, 0);
            assert_eq!(row, None);
        }

        #[test]
        fn all_rows_deleted_non_first_page_goes_to_previous_page() {
            let (page, row) = deletion_refresh_target_bulk(2, 2, 0, 3);
            assert_eq!(page, 2);
            assert_eq!(row, Some(usize::MAX));
        }

        #[test]
        fn middle_rows_deleted_selects_first_deleted_index() {
            let (page, row) = deletion_refresh_target_bulk(5, 2, 1, 0);
            assert_eq!(page, 0);
            assert_eq!(row, Some(1));
        }

        #[test]
        fn last_rows_deleted_selects_clamped_to_remaining_minus_one() {
            let (page, row) = deletion_refresh_target_bulk(5, 3, 2, 0);
            assert_eq!(page, 0);
            assert_eq!(row, Some(1));
        }

        #[test]
        fn single_row_deleted_from_middle_keeps_index() {
            let (page, row) = deletion_refresh_target_bulk(4, 1, 2, 1);
            assert_eq!(page, 1);
            assert_eq!(row, Some(2));
        }
    }

    mod bulk_delete_preview {
        use super::*;

        fn editable_state(database_type: DatabaseType) -> AppState {
            let mut state = AppState::new("test_project".to_string());
            let dsn = match database_type {
                DatabaseType::PostgreSQL => "postgres://localhost/test",
                DatabaseType::SQLite => "sqlite:///tmp/app.db",
            };
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("test-connection"),
                "test",
                database_type,
                dsn,
            );
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "SELECT * FROM users".to_string(),
                    vec!["id".to_string(), "name".to_string()],
                    vec![vec!["1".to_string(), "Alice".to_string()]],
                    10,
                    QuerySource::Preview,
                )));
            state.session.set_table_detail_raw(Some(Table {
                schema: "main".to_string(),
                name: "users".to_string(),
                columns: vec![
                    Column {
                        attributes: ColumnAttributes::PRIMARY_KEY,
                        ..sabiql_test_support::column::test_nullable_column("id", "INTEGER", 1)
                    },
                    sabiql_test_support::column::test_nullable_column("name", "TEXT", 2),
                ],
                primary_key: Some(vec!["id".to_string()]),
                ..sabiql_test_support::table::minimal("", "")
            }));
            state.query.pagination.reset_for_table("main", "users");
            state.result_interaction.stage_row(0);
            state
        }

        #[test]
        fn sqlite_database_type_uses_schema_free_delete_preview() {
            let state = editable_state(DatabaseType::SQLite);

            let result = build_bulk_delete_preview(&state, &AppServices::stub()).unwrap();

            assert_eq!(
                result.preview.sql,
                "DELETE FROM \"users\" WHERE \"id\" = '1'"
            );
        }

        #[test]
        fn sqlite_database_type_rejects_null_primary_key_value() {
            let mut state = editable_state(DatabaseType::SQLite);
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    "SELECT * FROM users".to_string(),
                    vec!["id".to_string(), "name".to_string()],
                    vec![vec![QueryValue::Null, QueryValue::text("Alice")]],
                    10,
                    QuerySource::Preview,
                )));

            let result = build_bulk_delete_preview(&state, &AppServices::stub());

            assert!(matches!(
                result,
                Err(EditGuardrailError::SqliteNullPrimaryKey)
            ));
        }
    }
}
