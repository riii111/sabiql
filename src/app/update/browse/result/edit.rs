use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::policy::write::write_update::build_pk_pairs;
use crate::app::update::action::{Action, InputTarget};

use crate::app::update::helpers::{EditGuardrailError, editable_preview_base};

fn is_jsonb_cell(state: &AppState) -> bool {
    let Some(col_idx) = state.result_interaction.selection().cell() else {
        return false;
    };
    let Some(td) = state.session.table_detail() else {
        return false;
    };
    // Ensure table_detail matches current preview target
    if td.schema != state.query.pagination.schema || td.name != state.query.pagination.table {
        return false;
    }
    td.columns
        .get(col_idx)
        .is_some_and(|c| c.data_type == "jsonb")
}

fn editable_cell_context(state: &AppState) -> Result<(usize, usize, String), EditGuardrailError> {
    let row_idx = state
        .result_interaction
        .selection()
        .row()
        .ok_or(EditGuardrailError::NoActiveRow)?;
    let col_idx = state
        .result_interaction
        .selection()
        .cell()
        .ok_or(EditGuardrailError::NoActiveCell)?;

    let (result, pk_cols) = editable_preview_base(state)?;

    let column_name = result
        .columns
        .get(col_idx)
        .ok_or(EditGuardrailError::ColumnIndexOutOfBounds)?;
    if pk_cols.iter().any(|pk| pk == column_name) {
        return Err(EditGuardrailError::PrimaryKeyColumnsReadOnly);
    }

    let row = result
        .rows
        .get(row_idx)
        .ok_or(EditGuardrailError::RowIndexOutOfBounds)?;
    if build_pk_pairs(&result.columns, row, pk_cols).is_none() {
        return Err(EditGuardrailError::StableKeyColumnsMissing);
    }

    let cell_value = row
        .get(col_idx)
        .ok_or(EditGuardrailError::CellIndexOutOfBounds)?
        .clone();

    Ok((row_idx, col_idx, cell_value))
}

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ResultEnterCellEdit => {
            if state.session.read_only {
                state
                    .messages
                    .set_error_at("Read-only mode: editing is disabled".to_string(), now);
                return Some(vec![]);
            }

            // JSONB columns open the dedicated detail modal instead of inline edit
            if is_jsonb_cell(state) {
                return Some(vec![Effect::DispatchActions(vec![Action::OpenJsonbDetail])]);
            }

            match editable_cell_context(state) {
                Ok((row_idx, col_idx, value)) => {
                    if state.result_interaction.cell_edit().row != Some(row_idx)
                        || state.result_interaction.cell_edit().col != Some(col_idx)
                    {
                        state
                            .result_interaction
                            .begin_cell_edit(row_idx, col_idx, value);
                        state.result_interaction.clear_write_preview();
                    }
                    state.modal.set_mode(InputMode::CellEdit);
                    Some(vec![])
                }
                Err(reason) => {
                    state.messages.set_error_at(reason.to_string(), now);
                    Some(vec![])
                }
            }
        }
        Action::ResultCancelCellEdit => {
            if state.result_interaction.cell_edit().has_pending_draft() {
                state.result_interaction.clear_write_preview();
            } else {
                state.result_interaction.discard_cell_edit();
            }
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }
        Action::ResultDiscardCellEdit => {
            state.result_interaction.discard_cell_edit();
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }
        Action::TextInput {
            target: InputTarget::ResultCellEdit,
            ch: c,
        } => {
            state
                .result_interaction
                .cell_edit_input_mut()
                .insert_char(*c);
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::ResultCellEdit,
        } => {
            state.result_interaction.cell_edit_input_mut().backspace();
            Some(vec![])
        }
        Action::TextDelete {
            target: InputTarget::ResultCellEdit,
        } => {
            state.result_interaction.cell_edit_input_mut().delete();
            Some(vec![])
        }
        Action::TextMoveCursor {
            target: InputTarget::ResultCellEdit,
            direction: m,
        } => {
            state
                .result_interaction
                .cell_edit_input_mut()
                .move_cursor(*m);
            Some(vec![])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::action::CursorMove;
    use crate::domain::{QueryResult, QuerySource, Table};
    use std::sync::Arc;

    mod cell_edit_entry_guardrails {
        use super::*;

        pub(super) fn minimal_users_table() -> Table {
            Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![],
                primary_key: Some(vec!["id".to_string()]),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        pub(super) fn preview_state_with_selection() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.query.set_current_result(Arc::new(QueryResult {
                query: String::new(),
                columns: vec!["id".to_string(), "name".to_string()],
                rows: vec![vec!["1".to_string(), "alice".to_string()]],
                row_count: 1,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.result_interaction.activate_cell(0, 1);
            state
        }

        #[test]
        fn re_entering_same_cell_with_pending_draft_preserves_draft() {
            let mut state = preview_state_with_selection();
            state
                .session
                .set_table_detail_raw(Some(minimal_users_table()));
            state
                .result_interaction
                .begin_cell_edit(0, 1, "alice".to_string());
            state
                .result_interaction
                .cell_edit_input_mut()
                .set_content("modified".to_string());
            state.modal.set_mode(InputMode::Normal);

            reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert_eq!(
                state.result_interaction.cell_edit().draft_value(),
                "modified"
            );
        }

        #[test]
        fn entering_different_cell_resets_draft() {
            let mut state = preview_state_with_selection();
            state
                .session
                .set_table_detail_raw(Some(minimal_users_table()));
            state
                .result_interaction
                .begin_cell_edit(0, 99, "stale".to_string());
            state
                .result_interaction
                .cell_edit_input_mut()
                .set_content("stale-modified".to_string());

            reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert_eq!(state.result_interaction.cell_edit().col, Some(1));
            assert_eq!(state.result_interaction.cell_edit().draft_value(), "alice");
        }

        #[test]
        fn stale_table_detail_blocks_cell_edit_entry() {
            let mut state = preview_state_with_selection();
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "posts".to_string(),
                owner: None,
                columns: vec![],
                primary_key: Some(vec!["id".to_string()]),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }));

            let effects = reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Table metadata does not match current preview target")
            );
        }
        #[test]
        fn cancel_without_changes_clears_cell_edit() {
            let mut state = preview_state_with_selection();
            state
                .session
                .set_table_detail_raw(Some(minimal_users_table()));
            state
                .result_interaction
                .begin_cell_edit(0, 1, "alice".to_string());
            state.modal.set_mode(InputMode::CellEdit);

            reduce(&mut state, &Action::ResultCancelCellEdit, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.result_interaction.cell_edit().is_active());
        }

        #[test]
        fn cancel_with_changes_preserves_draft() {
            let mut state = preview_state_with_selection();
            state
                .session
                .set_table_detail_raw(Some(minimal_users_table()));
            state
                .result_interaction
                .begin_cell_edit(0, 1, "alice".to_string());
            state
                .result_interaction
                .cell_edit_input_mut()
                .set_content("bob".to_string());
            state.modal.set_mode(InputMode::CellEdit);

            reduce(&mut state, &Action::ResultCancelCellEdit, Instant::now()).unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.result_interaction.cell_edit().is_active());
            assert_eq!(state.result_interaction.cell_edit().draft_value(), "bob");
        }
    }

    mod jsonb_dispatch {
        use super::*;
        use crate::domain::column::Column;

        fn state_with_jsonb_column() -> AppState {
            let mut state = cell_edit_entry_guardrails::preview_state_with_selection();
            let mut table = cell_edit_entry_guardrails::minimal_users_table();
            table.columns = vec![
                Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "name".to_string(),
                    data_type: "jsonb".to_string(),
                    nullable: true,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: 2,
                },
            ];
            state.session.set_table_detail_raw(Some(table));
            state
        }

        #[test]
        fn jsonb_cell_returns_dispatch_to_open_jsonb_detail() {
            let mut state = state_with_jsonb_column();

            let effects = reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::DispatchActions(actions) if matches!(actions.as_slice(), [Action::OpenJsonbDetail])
            ));
        }
    }

    mod read_only_guard {
        use super::*;

        #[test]
        fn read_only_blocks_cell_edit_entry() {
            let mut state = cell_edit_entry_guardrails::preview_state_with_selection();
            state
                .session
                .set_table_detail_raw(Some(cell_edit_entry_guardrails::minimal_users_table()));
            state.session.read_only = true;

            let effects = reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.messages.last_error.is_some());
        }
    }

    mod cell_edit_cursor_ops {
        use super::*;

        fn state_in_cell_edit(content: &str, cursor: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::CellEdit);
            state
                .result_interaction
                .begin_cell_edit(0, 0, content.to_string());
            state
                .result_interaction
                .cell_edit_input_mut()
                .set_cursor(cursor);
            state
        }

        #[test]
        fn delete_removes_char_at_cursor() {
            let mut state = state_in_cell_edit("abcd", 1);

            reduce(
                &mut state,
                &Action::TextDelete {
                    target: InputTarget::ResultCellEdit,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().draft_value(), "acd");
            assert_eq!(state.result_interaction.cell_edit().input.cursor(), 1);
        }

        #[test]
        fn delete_at_end_is_noop() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce(
                &mut state,
                &Action::TextDelete {
                    target: InputTarget::ResultCellEdit,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().draft_value(), "abc");
        }

        #[test]
        fn move_cursor_left_decrements() {
            let mut state = state_in_cell_edit("abc", 2);

            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Left,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input.cursor(), 1);
        }

        #[test]
        fn move_cursor_right_increments() {
            let mut state = state_in_cell_edit("abc", 1);

            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Right,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input.cursor(), 2);
        }

        #[test]
        fn move_cursor_home_jumps_to_start() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Home,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input.cursor(), 0);
        }

        #[test]
        fn move_cursor_end_jumps_to_end() {
            let mut state = state_in_cell_edit("abc", 0);

            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::End,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input.cursor(), 3);
        }

        #[test]
        fn input_inserts_at_cursor_not_at_end() {
            let mut state = state_in_cell_edit("ac", 1);

            reduce(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::ResultCellEdit,
                    ch: 'b',
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().draft_value(), "abc");
            assert_eq!(state.result_interaction.cell_edit().input.cursor(), 2);
        }

        #[test]
        fn backspace_removes_char_before_cursor() {
            let mut state = state_in_cell_edit("abc", 2);

            reduce(
                &mut state,
                &Action::TextBackspace {
                    target: InputTarget::ResultCellEdit,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().draft_value(), "ac");
            assert_eq!(state.result_interaction.cell_edit().input.cursor(), 1);
        }
    }
}
