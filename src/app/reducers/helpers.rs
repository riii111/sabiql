use crate::app::connection_setup_state::{ConnectionField, ConnectionSetupState};
use crate::app::services::AppServices;
use crate::app::state::AppState;
use crate::app::write_guardrails::{
    TargetSummary, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::app::write_update::build_pk_pairs;
use crate::domain::{QueryResult, QuerySource};

/// Resets all Result-pane view state (scroll, selection, staging, edit).
/// Used whenever the displayed result changes (adhoc success, history nav, etc.)
pub fn reset_result_view(state: &mut AppState) {
    state.ui.result_scroll_offset = 0;
    state.ui.result_horizontal_offset = 0;
    state.ui.result_selection.reset();
    state.cell_edit.clear();
    state.ui.staged_delete_rows.clear();
    state.pending_write_preview = None;
}

pub const ERR_EDITING_REQUIRES_PRIMARY_KEY: &str = "Editing requires a PRIMARY KEY.";
pub const ERR_DELETION_REQUIRES_PRIMARY_KEY: &str =
    "Deletion requires a PRIMARY KEY. This table has no PRIMARY KEY.";

/// Shared prerequisites for preview-cell write operations.
/// Entry checks in navigation and submit-time checks in query should both use this.
/// Row/column selection source is intentionally left to each caller:
/// navigation uses live selection, query submit uses cell_edit state.
pub fn editable_preview_base(state: &AppState) -> Result<(&QueryResult, &[String]), String> {
    if state.query.history_index.is_some() {
        return Err("Editing is unavailable while browsing history".to_string());
    }

    let result = state
        .query
        .current_result
        .as_ref()
        .map(|r| r.as_ref())
        .ok_or_else(|| "No result to edit".to_string())?;
    if result.source != QuerySource::Preview || result.is_error() {
        return Err("Only Preview results are editable".to_string());
    }

    if state.query.pagination.schema.is_empty() || state.query.pagination.table.is_empty() {
        return Err("Preview target table is unknown".to_string());
    }

    let table_detail = state
        .cache
        .table_detail
        .as_ref()
        .ok_or_else(|| "Table metadata not loaded".to_string())?;

    if table_detail.schema != state.query.pagination.schema
        || table_detail.name != state.query.pagination.table
    {
        return Err("Table metadata does not match current preview target".to_string());
    }

    let pk_cols = table_detail
        .primary_key
        .as_ref()
        .filter(|cols| !cols.is_empty())
        .map(|cols| cols.as_slice())
        .ok_or_else(|| ERR_EDITING_REQUIRES_PRIMARY_KEY.to_string())?;

    Ok((result, pk_cols))
}

pub fn build_bulk_delete_preview(
    state: &AppState,
    services: &AppServices,
) -> Result<(WritePreview, usize, Option<usize>), String> {
    if state.ui.staged_delete_rows.is_empty() {
        return Err("No rows staged for deletion".to_string());
    }
    if state.runtime.dsn.is_none() {
        return Err("No active connection".to_string());
    }
    if state.query.status != crate::app::query_execution::QueryStatus::Idle {
        return Err("Write is unavailable while query is running".to_string());
    }

    let (result, pk_cols) = editable_preview_base(state).map_err(|msg| {
        if msg == ERR_EDITING_REQUIRES_PRIMARY_KEY {
            ERR_DELETION_REQUIRES_PRIMARY_KEY.to_string()
        } else {
            msg
        }
    })?;

    let mut pk_pairs_per_row: Vec<Vec<(String, String)>> = Vec::new();
    for &row_idx in &state.ui.staged_delete_rows {
        let row = result
            .rows
            .get(row_idx)
            .ok_or_else(|| format!("Staged row index {} out of bounds", row_idx))?;
        let pairs = build_pk_pairs(&result.columns, row, pk_cols)
            .ok_or_else(|| "Stable key columns are not present in current result".to_string())?;
        pk_pairs_per_row.push(pairs);
    }

    let sql = services.sql_dialect.build_bulk_delete_sql(
        &state.query.pagination.schema,
        &state.query.pagination.table,
        &pk_pairs_per_row,
    );

    let staged_count = state.ui.staged_delete_rows.len();
    let first_deleted_idx = *state.ui.staged_delete_rows.iter().next().unwrap();
    let (target_page, target_row) = deletion_refresh_target_bulk(
        result.rows.len(),
        staged_count,
        first_deleted_idx,
        state.query.pagination.current_page,
    );

    let target = TargetSummary {
        schema: state.query.pagination.schema.clone(),
        table: state.query.pagination.table.clone(),
        key_values: pk_pairs_per_row.first().cloned().unwrap_or_default(),
    };
    let guardrail = evaluate_guardrails(true, true, Some(target.clone()));

    Ok((
        WritePreview {
            operation: WriteOperation::Delete,
            sql,
            target_summary: target,
            diff: vec![],
            guardrail,
        },
        target_page,
        target_row,
    ))
}

/// Computes the cursor target after bulk-deleting rows from a page.
///
/// `deleted_indices` is a sorted set of page-relative row indices that were deleted.
/// Returns `(target_page, target_row)` — same sentinel convention as `deletion_refresh_target`.
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
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

pub fn insert_char_at_cursor(s: &mut String, char_pos: usize, c: char) {
    let byte_idx = char_to_byte_index(s, char_pos);
    s.insert(byte_idx, c);
}

pub fn insert_str_at_cursor(s: &mut String, char_pos: usize, text: &str) -> usize {
    let byte_idx = char_to_byte_index(s, char_pos);
    s.insert_str(byte_idx, text);
    text.chars().count()
}

pub fn validate_field(state: &mut ConnectionSetupState, field: ConnectionField) {
    state.validation_errors.remove(&field);

    match field {
        ConnectionField::Host => {
            if state.host.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::Port => {
            if state.port.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            } else {
                match state.port.parse::<u16>() {
                    Err(_) => {
                        state
                            .validation_errors
                            .insert(field, "Invalid port".to_string());
                    }
                    Ok(0) => {
                        state
                            .validation_errors
                            .insert(field, "Port must be > 0".to_string());
                    }
                    Ok(_) => {}
                }
            }
        }
        ConnectionField::Database => {
            if state.database.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::User => {
            if state.user.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::Name => {
            let name = state.name.trim();
            if name.is_empty() {
                state
                    .validation_errors
                    .insert(field, "Name is required".to_string());
            } else if name.chars().count() > 50 {
                state
                    .validation_errors
                    .insert(field, "Name must be 50 characters or less".to_string());
            }
        }
        ConnectionField::Password | ConnectionField::SslMode => {
            // Optional fields, no validation needed
        }
    }
}

pub fn validate_all(state: &mut ConnectionSetupState) {
    for field in ConnectionField::all() {
        validate_field(state, *field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod insert_str_at_cursor_tests {
        use super::*;

        #[test]
        fn insert_str_at_empty_string() {
            let mut s = String::new();

            let count = insert_str_at_cursor(&mut s, 0, "abc");

            assert_eq!(s, "abc");
            assert_eq!(count, 3);
        }

        #[test]
        fn insert_str_at_beginning() {
            let mut s = "hello".to_string();

            let count = insert_str_at_cursor(&mut s, 0, "xy");

            assert_eq!(s, "xyhello");
            assert_eq!(count, 2);
        }

        #[test]
        fn insert_str_at_middle() {
            let mut s = "abcd".to_string();

            let count = insert_str_at_cursor(&mut s, 2, "XX");

            assert_eq!(s, "abXXcd");
            assert_eq!(count, 2);
        }

        #[test]
        fn insert_str_at_end() {
            let mut s = "abcd".to_string();

            let count = insert_str_at_cursor(&mut s, 4, "!");

            assert_eq!(s, "abcd!");
            assert_eq!(count, 1);
        }

        #[test]
        fn insert_str_with_multibyte() {
            let mut s = "abc".to_string();

            let count = insert_str_at_cursor(&mut s, 1, "日本");

            assert_eq!(s, "a日本bc");
            assert_eq!(count, 2);
        }
    }

    mod validate_field_name {
        use super::*;

        #[test]
        fn empty_name_sets_error() {
            let mut state = ConnectionSetupState {
                name: "".to_string(),
                ..Default::default()
            };

            validate_field(&mut state, ConnectionField::Name);

            assert_eq!(
                state.validation_errors.get(&ConnectionField::Name),
                Some(&"Name is required".to_string())
            );
        }

        #[test]
        fn whitespace_only_name_sets_error() {
            let mut state = ConnectionSetupState {
                name: "   ".to_string(),
                ..Default::default()
            };

            validate_field(&mut state, ConnectionField::Name);

            assert_eq!(
                state.validation_errors.get(&ConnectionField::Name),
                Some(&"Name is required".to_string())
            );
        }

        #[rstest]
        #[case("a".repeat(50), false)]
        #[case("a".repeat(51), true)]
        fn name_length_validation(#[case] name: String, #[case] expect_error: bool) {
            let mut state = ConnectionSetupState {
                name,
                ..Default::default()
            };

            validate_field(&mut state, ConnectionField::Name);

            if expect_error {
                assert_eq!(
                    state.validation_errors.get(&ConnectionField::Name),
                    Some(&"Name must be 50 characters or less".to_string())
                );
            } else {
                assert!(!state.validation_errors.contains_key(&ConnectionField::Name));
            }
        }

        #[test]
        fn valid_name_clears_previous_error() {
            let mut state = ConnectionSetupState {
                name: "".to_string(),
                ..Default::default()
            };
            validate_field(&mut state, ConnectionField::Name);
            assert!(state.validation_errors.contains_key(&ConnectionField::Name));

            state.name = "Valid Name".to_string();
            validate_field(&mut state, ConnectionField::Name);

            assert!(!state.validation_errors.contains_key(&ConnectionField::Name));
        }
    }

    mod delete_refresh_target {
        fn deletion_refresh_target(
            row_count: usize,
            selected_row: usize,
            current_page: usize,
        ) -> (usize, Option<usize>) {
            if row_count <= 1 {
                if current_page > 0 {
                    (current_page - 1, Some(usize::MAX))
                } else {
                    (0, None)
                }
            } else if selected_row < row_count - 1 {
                (current_page, Some(selected_row))
            } else {
                (current_page, Some(row_count - 2))
            }
        }

        #[test]
        fn single_row_first_page_clears_selection() {
            let (page, row) = deletion_refresh_target(1, 0, 0);
            assert_eq!(page, 0);
            assert_eq!(row, None);
        }

        #[test]
        fn single_row_non_first_page_goes_previous_page_last_row() {
            let (page, row) = deletion_refresh_target(1, 0, 2);
            assert_eq!(page, 1);
            assert_eq!(row, Some(usize::MAX));
        }

        #[test]
        fn middle_row_keeps_same_index() {
            let (page, row) = deletion_refresh_target(3, 1, 4);
            assert_eq!(page, 4);
            assert_eq!(row, Some(1));
        }

        #[test]
        fn last_row_selects_previous_row() {
            let (page, row) = deletion_refresh_target(3, 2, 4);
            assert_eq!(page, 4);
            assert_eq!(row, Some(1));
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
}
