use std::sync::Arc;

use unicode_casefold::UnicodeCaseFold;

use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::domain::connection::{
    ConnectionEndpoint, ConnectionName, ConnectionNameError, MySqlConnectionConfig, MySqlTransport,
    SqliteConnectionConfig,
};
use crate::domain::{QueryResult, QueryValue};
use crate::model::app_state::AppState;
use crate::model::connection::setup::{ConnectionField, ConnectionSetupState};
use crate::policy::column::column_read_only_reason;
use crate::policy::write::inline_cell_edit::InlineCellEditError;
use crate::policy::write::write_guardrails::{
    PreviewWriteability, StableRowIdentity, TargetSummary, WriteOperation, WritePreview,
    evaluate_guardrails, preview_writeability_for_result, stable_row_identity_for_preview,
};
use crate::ports::outbound::ClipboardError;
use crate::update::action::Action;

pub(in crate::update) fn reject_pending_mysql_connection_probe(state: &mut AppState) -> bool {
    if state.session.pending_mysql_connection_probe().is_none() {
        return false;
    }

    state
        .messages
        .set_error("Connection switch in progress".to_string());
    true
}

pub(in crate::update) fn clipboard_unavailable() -> Action {
    Action::CopyFailed(ClipboardError::Unavailable("Clipboard unavailable".into()))
}

pub(in crate::update) fn metadata_reload_effects(
    state: &mut AppState,
    endpoint: &Arc<ConnectionEndpoint>,
) -> Vec<Effect> {
    let run_id = state.session.begin_reload();
    state.table_detail_cache.clear();

    vec![
        Effect::CancelMetadataTasks,
        Effect::FetchMetadata {
            endpoint: Arc::clone(endpoint),
            run_id,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(in crate::update) enum EditGuardrailError {
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
    #[error("Deletion requires a PRIMARY KEY.")]
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
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),
    #[error("No semantic changes to write")]
    NoSemanticChanges,
    #[error(transparent)]
    InlineCellEdit(#[from] InlineCellEditError),
    #[error("SQLite writes require non-NULL primary key values")]
    SqliteNullPrimaryKey,
    #[error("{0}")]
    GuardrailBlocked(String),
}

pub(in crate::update) struct BulkDeletePreviewResult {
    pub preview: WritePreview,
    pub target_page: usize,
    pub target_row: Option<usize>,
}

pub(in crate::update) fn reject_sqlite_null_pk(
    database_type: DatabaseType,
    pk_pairs: &[(String, QueryValue)],
) -> Result<(), EditGuardrailError> {
    let rejects_null_identity = match database_type {
        DatabaseType::SQLite => pk_pairs
            .iter()
            .any(|(_, value)| matches!(value, QueryValue::Null)),
        DatabaseType::PostgreSQL | DatabaseType::MySQL => false,
    };
    if rejects_null_identity {
        return Err(EditGuardrailError::SqliteNullPrimaryKey);
    }
    Ok(())
}

// Entry checks in navigation and submit-time checks in query should both use this.
// Row/column selection source is intentionally left to each caller:
// navigation uses live selection, query submit uses cell_edit state.
pub(in crate::update) fn editable_preview_base(
    state: &AppState,
) -> Result<(&QueryResult, StableRowIdentity), EditGuardrailError> {
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

    if !state.query.pagination.matches_table(table_detail) {
        return Err(EditGuardrailError::StaleTableMetadata);
    }
    match preview_writeability_for_result(table_detail, result) {
        PreviewWriteability::Writable => {}
        PreviewWriteability::ReadOnly(reason) => {
            return Err(EditGuardrailError::ReadOnlyPreviewTarget(reason));
        }
        PreviewWriteability::MissingStableRowIdentity => {
            return Err(EditGuardrailError::EditingRequiresPrimaryKey);
        }
    }
    let identity = stable_row_identity_for_preview(table_detail, result)
        .ok_or(EditGuardrailError::EditingRequiresPrimaryKey)?;

    Ok((result, identity))
}

pub(in crate::update) fn ensure_column_writable(
    state: &AppState,
    column_name: &str,
    identity: &StableRowIdentity,
) -> Result<(), EditGuardrailError> {
    if identity.is_primary_key_column(column_name) {
        return Err(EditGuardrailError::PrimaryKeyColumnsReadOnly);
    }

    if let Some(column) = state.session.table_detail().and_then(|table| {
        table
            .columns
            .iter()
            .find(|column| column.name == column_name)
    }) && column.is_read_only()
    {
        let reason = column_read_only_reason(column).unwrap_or("read-only");
        return Err(EditGuardrailError::ReadOnlyColumn(format!(
            "{column_name} ({reason})"
        )));
    }

    Ok(())
}

pub(in crate::update) fn build_bulk_delete_preview(
    state: &AppState,
) -> Result<BulkDeletePreviewResult, EditGuardrailError> {
    if state.result_interaction.staged_delete_rows().is_empty() {
        return Err(EditGuardrailError::NoRowsStagedForDeletion);
    }
    if state.session.endpoint().is_none() {
        return Err(EditGuardrailError::NoActiveConnection);
    }
    if state.query.is_running() {
        return Err(EditGuardrailError::WriteUnavailableWhileQueryRunning);
    }

    let (result, identity) = editable_preview_base(state).map_err(|err| match err {
        EditGuardrailError::EditingRequiresPrimaryKey => {
            EditGuardrailError::DeletionRequiresPrimaryKey
        }
        other => other,
    })?;

    let mut predicate_pairs_per_row: Vec<Vec<(String, QueryValue)>> = Vec::new();
    let mut target_pairs = Vec::new();
    for &row_idx in state.result_interaction.staged_delete_rows() {
        if row_idx >= result.values().len() {
            return Err(EditGuardrailError::StagedRowIndexOutOfBounds(row_idx));
        }
        let identity_pairs = identity
            .identity_pairs_for_row(result, row_idx)
            .ok_or(EditGuardrailError::StableKeyColumnsMissing)?;
        reject_sqlite_null_pk(
            state.session.active_database_type_or_default(),
            &identity_pairs,
        )?;
        if target_pairs.is_empty() {
            target_pairs.clone_from(&identity_pairs);
        }
        predicate_pairs_per_row.push(identity_pairs);
    }

    let sql = state
        .session
        .active_database_type_or_default()
        .dialect()
        .bulk_delete_sql(
            state.query.pagination.schema(),
            state.query.pagination.table(),
            &predicate_pairs_per_row,
        );

    let staged_count = state.result_interaction.staged_delete_rows().len();
    let first_deleted_idx = *state
        .result_interaction
        .staged_delete_rows()
        .iter()
        .next()
        .unwrap();
    let (target_page, target_row) = deletion_refresh_target_bulk(
        result.data_row_count(),
        staged_count,
        first_deleted_idx,
        state.query.pagination.current_page(),
    );

    let target = TargetSummary {
        schema: state.query.pagination.schema().to_string(),
        table: state.query.pagination.table().to_string(),
        key_values: target_pairs,
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

pub(in crate::update) fn deletion_refresh_target_bulk(
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

pub(in crate::update) fn find_text_matches(content: &str, query: &str) -> Vec<usize> {
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

pub(in crate::update) fn validate_field(state: &mut ConnectionSetupState, field: ConnectionField) {
    state.clear_validation_error(field);
    if let Some(other_field) = match field {
        ConnectionField::SslCert => Some(ConnectionField::SslKey),
        ConnectionField::SslKey => Some(ConnectionField::SslCert),
        _ => None,
    } {
        state.clear_validation_error(other_field);
    }

    if let Some(max_chars) = field.max_chars() {
        let length = state.field_value(field).chars().count();
        if length > max_chars {
            state
                .validation_errors
                .insert(field, format!("Must be {max_chars} characters or less"));
            return;
        }
    }

    let engine = state.database_type();
    match field {
        ConnectionField::Name => match ConnectionName::validate(text_input_content(state, field)) {
            Ok(()) => {}
            Err(ConnectionNameError::Empty) => {
                state.set_validation_error(field, "Name is required")
            }
            Err(ConnectionNameError::TooLong { max, .. }) => {
                state.set_validation_error(field, format!("Must be {max} characters or less"));
            }
        },
        ConnectionField::Port
            if engine == DatabaseType::PostgreSQL
                || engine == DatabaseType::MySQL
                    && state.mysql_transport() == MySqlTransport::Tcp =>
        {
            validate_port(state);
        }
        ConnectionField::Database if engine == DatabaseType::PostgreSQL => {
            require_non_empty(state, field, "Required");
        }
        ConnectionField::SqlitePath if engine == DatabaseType::SQLite => {
            let path = text_input_content(state, field).to_string();
            if let Err(error) = SqliteConnectionConfig::new(path) {
                state.record_sqlite_config_error(error);
            }
        }
        _ if engine == DatabaseType::MySQL => validate_mysql_field(state, field),
        _ => {}
    }
}

pub(in crate::update) fn validate_all(state: &mut ConnectionSetupState) {
    state.retain_validation_errors_for_visible_fields();
    for field in state.visible_fields() {
        validate_field(state, field);
    }
}

fn validate_mysql_field(state: &mut ConnectionSetupState, field: ConnectionField) {
    match field {
        ConnectionField::Host if state.mysql_transport() == MySqlTransport::Tcp => {
            let host = text_input_content(state, field).trim();
            if host.is_empty() {
                state.set_validation_error(field, "Required");
            } else if !MySqlConnectionConfig::is_valid_host(host) {
                state.set_validation_error(field, "Invalid host");
            }
        }
        ConnectionField::Database | ConnectionField::User => {
            require_non_empty(state, field, "Required");
        }
        ConnectionField::TransportPath if state.mysql_transport().requires_path() => {
            let path = text_input_content(state, field);
            if path.trim().is_empty() {
                state.set_validation_error(field, "Required");
            } else if !MySqlTransport::is_valid_path(path) {
                state.set_validation_error(field, "Invalid path");
            }
        }
        ConnectionField::TransportPath | ConnectionField::ServerPublicKeyPath => {
            let path = text_input_content(state, field);
            if path.chars().any(char::is_control) {
                state.set_validation_error(field, "Invalid path");
            }
        }
        ConnectionField::SslCa => {
            let path = text_input_content(state, field);
            if state.mysql_ssl_mode().uses_ca() && path.trim().is_empty() {
                state.set_validation_error(field, "Required for this TLS mode");
            } else if path.chars().any(char::is_control) {
                state.set_validation_error(field, "Invalid path");
            }
        }
        ConnectionField::SslCert | ConnectionField::SslKey => {
            let path = text_input_content(state, field).to_string();
            if path.chars().any(char::is_control) {
                state.set_validation_error(field, "Invalid path");
            }
            let other_field = match field {
                ConnectionField::SslCert => ConnectionField::SslKey,
                ConnectionField::SslKey => ConnectionField::SslCert,
                _ => unreachable!(),
            };
            let other_path = text_input_content(state, other_field).to_string();
            if path.trim().is_empty() != other_path.trim().is_empty() {
                state.set_validation_error(field, "Both client paths are required");
                state.set_validation_error(other_field, "Both client paths are required");
            }
        }
        ConnectionField::CleartextAuth => {
            if state.cleartext_auth_plugin_enabled()
                && !state.mysql_ssl_mode().allows_cleartext_auth()
            {
                state.set_validation_error(field, "Requires TLS");
            }
        }
        ConnectionField::SslMode => {
            if !state
                .mysql_transport()
                .allows_ssl_mode(state.mysql_ssl_mode())
            {
                state.set_validation_error(field, "Named pipes do not support TLS");
            }
        }
        ConnectionField::DatabaseType
        | ConnectionField::Name
        | ConnectionField::Transport
        | ConnectionField::Host
        | ConnectionField::Port
        | ConnectionField::Password
        | ConnectionField::SqlitePath => {}
    }
}

fn validate_port(state: &mut ConnectionSetupState) {
    let port = text_input_content(state, ConnectionField::Port).trim();
    if port.is_empty() {
        state.set_validation_error(ConnectionField::Port, "Required");
    } else {
        match port.parse::<u16>() {
            Err(_) => {
                state.set_validation_error(ConnectionField::Port, "Invalid port");
            }
            Ok(0) => {
                state.set_validation_error(ConnectionField::Port, "Port must be > 0");
            }
            Ok(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Column;
    use std::sync::Arc;

    use crate::domain::connection::ConnectionId;
    use crate::domain::connection::MySqlSslMode;
    use crate::domain::{ColumnAttributes, DatabaseType, QuerySource, Table};
    use crate::test_support::endpoint;
    use rstest::rstest;

    mod validate_field_name {
        use super::*;
        use crate::model::shared::text_input::TextInputState;

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
                Some(
                    "In-memory SQLite databases cannot retain contents because sabiql starts sqlite3 per operation; use a temporary file"
                )
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
                Some("SQLite URI filenames are not supported; use a regular file path")
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

    #[test]
    fn mysql_cleartext_auth_requires_tls() {
        let mut state = ConnectionSetupState::default();
        state.set_database_type(DatabaseType::MySQL);
        state.enable_cleartext_plugin = true;

        validate_all(&mut state);

        assert_eq!(
            state.validation_error(ConnectionField::CleartextAuth),
            Some("Requires TLS")
        );

        state.mysql_ssl_mode = MySqlSslMode::Required;
        validate_all(&mut state);

        assert_eq!(state.validation_error(ConnectionField::CleartextAuth), None);
    }

    #[test]
    fn validating_filled_client_key_clears_stale_pair_error() {
        let mut state = ConnectionSetupState::default();
        state.set_database_type(DatabaseType::MySQL);
        state
            .input_mut(ConnectionField::SslCert)
            .unwrap()
            .set_content("client.pem".to_string());
        validate_field(&mut state, ConnectionField::SslCert);

        state
            .input_mut(ConnectionField::SslKey)
            .unwrap()
            .set_content("client-key.pem".to_string());
        validate_field(&mut state, ConnectionField::SslKey);

        assert_eq!(state.validation_error(ConnectionField::SslCert), None);
        assert_eq!(state.validation_error(ConnectionField::SslKey), None);
    }

    #[test]
    fn empty_or_complete_client_certificate_pair_is_accepted() {
        let mut state = ConnectionSetupState::default();
        state.set_database_type(DatabaseType::MySQL);
        validate_all(&mut state);
        assert_eq!(state.validation_error(ConnectionField::SslCert), None);
        assert_eq!(state.validation_error(ConnectionField::SslKey), None);

        state
            .input_mut(ConnectionField::SslCert)
            .unwrap()
            .set_content("client.pem".to_string());
        state
            .input_mut(ConnectionField::SslKey)
            .unwrap()
            .set_content("client-key.pem".to_string());
        validate_all(&mut state);
        assert_eq!(state.validation_error(ConnectionField::SslCert), None);
        assert_eq!(state.validation_error(ConnectionField::SslKey), None);
    }

    mod validate_all_by_engine {
        use super::*;
        use std::collections::HashMap;

        fn form(
            database_type: DatabaseType,
            inputs: &[(ConnectionField, &str)],
        ) -> ConnectionSetupState {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(database_type);
            for (field, value) in inputs {
                state
                    .input_mut(*field)
                    .expect("connection field is a text input")
                    .set_content((*value).to_string());
            }
            state
        }

        fn errors_after_validate_all(
            mut state: ConnectionSetupState,
        ) -> HashMap<ConnectionField, String> {
            validate_all(&mut state);
            state.validation_errors
        }

        fn errors(expected: &[(ConnectionField, &str)]) -> HashMap<ConnectionField, String> {
            expected
                .iter()
                .map(|(field, message)| (*field, (*message).to_string()))
                .collect()
        }

        const EMPTY_HOST_AND_PORT: &[(ConnectionField, &str)] =
            &[(ConnectionField::Host, ""), (ConnectionField::Port, "")];

        #[test]
        fn postgres_requires_name_port_and_database_but_not_host_or_user() {
            let state = form(DatabaseType::PostgreSQL, EMPTY_HOST_AND_PORT);

            assert_eq!(
                errors_after_validate_all(state),
                errors(&[
                    (ConnectionField::Name, "Name is required"),
                    (ConnectionField::Port, "Required"),
                    (ConnectionField::Database, "Required"),
                ])
            );
        }

        #[test]
        fn postgres_accepts_any_host_text_and_reports_an_invalid_port() {
            let state = form(
                DatabaseType::PostgreSQL,
                &[
                    (ConnectionField::Name, "pg"),
                    (ConnectionField::Host, "db example"),
                    (ConnectionField::Port, "abc"),
                    (ConnectionField::Database, "app"),
                ],
            );

            assert_eq!(
                errors_after_validate_all(state),
                errors(&[(ConnectionField::Port, "Invalid port")])
            );
        }

        #[test]
        fn mysql_tcp_requires_name_host_port_database_and_user() {
            let state = form(
                DatabaseType::MySQL,
                &[
                    (ConnectionField::Name, " "),
                    (ConnectionField::Host, " "),
                    (ConnectionField::Port, ""),
                    (ConnectionField::User, " "),
                ],
            );

            assert_eq!(
                errors_after_validate_all(state),
                errors(&[
                    (ConnectionField::Name, "Name is required"),
                    (ConnectionField::Host, "Required"),
                    (ConnectionField::Port, "Required"),
                    (ConnectionField::Database, "Required"),
                    (ConnectionField::User, "Required"),
                ])
            );
        }

        #[test]
        fn mysql_tcp_reports_invalid_host_port_and_paths() {
            let mut state = form(
                DatabaseType::MySQL,
                &[
                    (ConnectionField::Name, "mysql"),
                    (ConnectionField::Host, "db example"),
                    (ConnectionField::Port, "0"),
                    (ConnectionField::Database, "app"),
                    (ConnectionField::User, "root"),
                    (ConnectionField::ServerPublicKeyPath, "server\n.pem"),
                    (ConnectionField::SslCert, "client.pem"),
                ],
            );
            state.enable_cleartext_plugin = true;

            assert_eq!(
                errors_after_validate_all(state),
                errors(&[
                    (ConnectionField::Host, "Invalid host"),
                    (ConnectionField::Port, "Port must be > 0"),
                    (ConnectionField::CleartextAuth, "Requires TLS"),
                    (ConnectionField::ServerPublicKeyPath, "Invalid path"),
                    (ConnectionField::SslCert, "Both client paths are required"),
                    (ConnectionField::SslKey, "Both client paths are required"),
                ])
            );
        }

        #[rstest]
        #[case::verify_ca(MySqlSslMode::VerifyCa)]
        #[case::verify_identity(MySqlSslMode::VerifyIdentity)]
        fn mysql_ca_verification_requires_a_valid_ca_path(#[case] ssl_mode: MySqlSslMode) {
            let mut missing = form(
                DatabaseType::MySQL,
                &[
                    (ConnectionField::Name, "mysql"),
                    (ConnectionField::Database, "app"),
                    (ConnectionField::User, "root"),
                ],
            );
            missing.mysql_ssl_mode = ssl_mode;
            let mut invalid = missing.clone();
            invalid
                .input_mut(ConnectionField::SslCa)
                .unwrap()
                .set_content("ca\n.pem".to_string());

            assert_eq!(
                errors_after_validate_all(missing),
                errors(&[(ConnectionField::SslCa, "Required for this TLS mode")])
            );
            assert_eq!(
                errors_after_validate_all(invalid),
                errors(&[(ConnectionField::SslCa, "Invalid path")])
            );
        }

        #[cfg(unix)]
        #[test]
        fn mysql_unix_socket_requires_a_path_instead_of_host_and_port() {
            let mut missing = form(DatabaseType::MySQL, EMPTY_HOST_AND_PORT);
            missing.mysql_transport = MySqlTransport::UnixSocket;
            let mut invalid = missing.clone();
            invalid
                .input_mut(ConnectionField::TransportPath)
                .unwrap()
                .set_content("/tmp/mysql\n.sock".to_string());
            let mut valid = form(
                DatabaseType::MySQL,
                &[
                    (ConnectionField::Name, "mysql"),
                    (ConnectionField::Host, " "),
                    (ConnectionField::Port, "not-a-port"),
                    (ConnectionField::Database, "app"),
                    (ConnectionField::User, "root"),
                    (ConnectionField::TransportPath, "/run/mysqld/mysqld.sock"),
                ],
            );
            valid.mysql_transport = MySqlTransport::UnixSocket;

            assert_eq!(
                errors_after_validate_all(missing),
                errors(&[
                    (ConnectionField::Name, "Name is required"),
                    (ConnectionField::TransportPath, "Required"),
                    (ConnectionField::Database, "Required"),
                    (ConnectionField::User, "Required"),
                ])
            );
            assert_eq!(
                errors_after_validate_all(invalid),
                errors(&[
                    (ConnectionField::Name, "Name is required"),
                    (ConnectionField::TransportPath, "Invalid path"),
                    (ConnectionField::Database, "Required"),
                    (ConnectionField::User, "Required"),
                ])
            );
            assert_eq!(errors_after_validate_all(valid), errors(&[]));
        }

        #[cfg(windows)]
        #[test]
        fn mysql_named_pipe_requires_a_path_and_rejects_tls_modes_beyond_preferred() {
            let mut state = form(DatabaseType::MySQL, EMPTY_HOST_AND_PORT);
            state.mysql_transport = MySqlTransport::NamedPipe;
            state.mysql_ssl_mode = MySqlSslMode::Required;

            assert_eq!(
                errors_after_validate_all(state),
                errors(&[
                    (ConnectionField::Name, "Name is required"),
                    (ConnectionField::TransportPath, "Required"),
                    (ConnectionField::Database, "Required"),
                    (ConnectionField::User, "Required"),
                    (ConnectionField::SslMode, "Named pipes do not support TLS"),
                ])
            );
        }

        #[test]
        fn sqlite_requires_only_name_and_path() {
            let state = form(
                DatabaseType::SQLite,
                &[
                    (ConnectionField::Host, "db example"),
                    (ConnectionField::Port, "abc"),
                    (ConnectionField::SqlitePath, "   "),
                ],
            );

            assert_eq!(
                errors_after_validate_all(state),
                errors(&[
                    (ConnectionField::Name, "Name is required"),
                    (ConnectionField::SqlitePath, "Required"),
                ])
            );
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
        use crate::test_support;

        use super::*;

        fn sqlite_editable_state() -> AppState {
            let mut state = AppState::new("test_project".to_string());
            state.session.activate_connection(
                &ConnectionId::from_string("test-connection"),
                "test",
                &endpoint::sqlite("sqlite:///tmp/app.db"),
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
                        ..test_support::column::test_nullable_column("id", "INTEGER", 1)
                    },
                    test_support::column::test_nullable_column("name", "TEXT", 2),
                ],
                primary_key: Some(vec!["id".to_string()]),
                ..test_support::table::minimal("", "")
            }));
            state.query.pagination.reset_for_table("main", "users");
            state.result_interaction.stage_row(0);
            state
        }

        #[test]
        fn sqlite_database_type_uses_schema_free_delete_preview() {
            let state = sqlite_editable_state();

            let result = build_bulk_delete_preview(&state).unwrap();

            assert_eq!(
                result.preview.sql,
                "DELETE FROM \"users\"\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn sqlite_table_without_primary_key_cannot_build_delete_preview() {
            let mut state = sqlite_editable_state();
            let mut detail = state.session.table_detail().cloned().expect("table detail");
            detail.primary_key = None;
            state.session.set_table_detail_raw(Some(detail));

            assert!(matches!(
                build_bulk_delete_preview(&state),
                Err(EditGuardrailError::DeletionRequiresPrimaryKey)
            ));
        }

        #[test]
        fn sqlite_without_rowid_table_uses_primary_key_for_delete_preview() {
            let mut state = sqlite_editable_state();
            let mut detail = state.session.table_detail().cloned().expect("table detail");
            detail.kind_info.without_rowid = true;
            state.session.set_table_detail_raw(Some(detail));

            let result = build_bulk_delete_preview(&state).unwrap();

            assert_eq!(
                result.preview.sql,
                "DELETE FROM \"users\"\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn sqlite_database_type_rejects_null_primary_key_value() {
            let mut state = sqlite_editable_state();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    "SELECT * FROM users".to_string(),
                    vec!["id".to_string(), "name".to_string()],
                    vec![vec![QueryValue::Null, QueryValue::text("Alice")]],
                    10,
                    QuerySource::Preview,
                )));

            let result = build_bulk_delete_preview(&state);

            assert!(matches!(
                result,
                Err(EditGuardrailError::SqliteNullPrimaryKey)
            ));
        }
    }

    mod text_search {
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
}
