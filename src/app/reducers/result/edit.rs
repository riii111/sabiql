use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;
use crate::app::write_update::build_pk_pairs;

use super::super::helpers::editable_preview_base;

fn editable_cell_context(state: &AppState) -> Result<(usize, usize, String), String> {
    let row_idx = state
        .ui
        .result_selection
        .row()
        .ok_or_else(|| "No active row".to_string())?;
    let col_idx = state
        .ui
        .result_selection
        .cell()
        .ok_or_else(|| "No active cell".to_string())?;

    let (result, pk_cols) = editable_preview_base(state)?;

    let column_name = result
        .columns
        .get(col_idx)
        .ok_or_else(|| "Column index out of bounds".to_string())?;
    if pk_cols.iter().any(|pk| pk == column_name) {
        return Err("Primary key columns are read-only".to_string());
    }

    let row = result
        .rows
        .get(row_idx)
        .ok_or_else(|| "Row index out of bounds".to_string())?;
    if build_pk_pairs(&result.columns, row, pk_cols).is_none() {
        return Err("Stable key columns are not present in current result".to_string());
    }

    let cell_value = row
        .get(col_idx)
        .ok_or_else(|| "Cell index out of bounds".to_string())?
        .clone();

    Ok((row_idx, col_idx, cell_value))
}

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ResultEnterCellEdit => match editable_cell_context(state) {
            Ok((row_idx, col_idx, value)) => {
                if state.cell_edit.row != Some(row_idx) || state.cell_edit.col != Some(col_idx) {
                    state.cell_edit.begin(row_idx, col_idx, value);
                    state.pending_write_preview = None;
                }
                state.ui.input_mode = InputMode::CellEdit;
                Some(vec![])
            }
            Err(reason) => {
                state.messages.set_error_at(reason, now);
                Some(vec![])
            }
        },
        Action::ResultCancelCellEdit => {
            state.pending_write_preview = None;
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ResultDiscardCellEdit => {
            state.cell_edit.clear();
            state.pending_write_preview = None;
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ResultCellEditInput(c) => {
            state.cell_edit.input.insert_char(*c);
            Some(vec![])
        }
        Action::ResultCellEditBackspace => {
            state.cell_edit.input.backspace();
            Some(vec![])
        }
        Action::ResultCellEditDelete => {
            state.cell_edit.input.delete();
            Some(vec![])
        }
        Action::ResultCellEditMoveCursor(m) => {
            state.cell_edit.input.move_cursor(*m);
            Some(vec![])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::action::CursorMove;
    use crate::domain::{QueryResult, QuerySource, Table};
    use std::sync::Arc;

    mod cell_edit_entry_guardrails {
        use super::*;

        fn minimal_users_table() -> Table {
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

        fn preview_state_with_selection() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.query.current_result = Some(Arc::new(QueryResult {
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
            state.ui.result_selection.enter_row(0);
            state.ui.result_selection.enter_cell(1);
            state
        }

        #[test]
        fn re_entering_same_cell_with_pending_draft_preserves_draft() {
            let mut state = preview_state_with_selection();
            state.cache.table_detail = Some(minimal_users_table());
            state.cell_edit.begin(0, 1, "alice".to_string());
            state.cell_edit.input.set_content("modified".to_string());
            state.ui.input_mode = InputMode::Normal;

            reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert_eq!(state.cell_edit.draft_value(), "modified");
        }

        #[test]
        fn entering_different_cell_resets_draft() {
            let mut state = preview_state_with_selection();
            state.cache.table_detail = Some(minimal_users_table());
            state.cell_edit.begin(0, 99, "stale".to_string());
            state
                .cell_edit
                .input
                .set_content("stale-modified".to_string());

            reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert_eq!(state.ui.input_mode, InputMode::CellEdit);
            assert_eq!(state.cell_edit.col, Some(1));
            assert_eq!(state.cell_edit.draft_value(), "alice");
        }

        #[test]
        fn stale_table_detail_blocks_cell_edit_entry() {
            let mut state = preview_state_with_selection();
            state.cache.table_detail = Some(Table {
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
            });

            let effects = reduce(&mut state, &Action::ResultEnterCellEdit, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Table metadata does not match current preview target")
            );
        }
    }

    mod cell_edit_cursor_ops {
        use super::*;

        fn state_in_cell_edit(content: &str, cursor: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::CellEdit;
            state.cell_edit.begin(0, 0, content.to_string());
            state.cell_edit.input.set_cursor(cursor);
            state
        }

        #[test]
        fn delete_removes_char_at_cursor() {
            let mut state = state_in_cell_edit("abcd", 1);

            reduce(&mut state, &Action::ResultCellEditDelete, Instant::now());

            assert_eq!(state.cell_edit.draft_value(), "acd");
            assert_eq!(state.cell_edit.input.cursor(), 1);
        }

        #[test]
        fn delete_at_end_is_noop() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce(&mut state, &Action::ResultCellEditDelete, Instant::now());

            assert_eq!(state.cell_edit.draft_value(), "abc");
        }

        #[test]
        fn move_cursor_left_decrements() {
            let mut state = state_in_cell_edit("abc", 2);

            reduce(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::Left),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 1);
        }

        #[test]
        fn move_cursor_right_increments() {
            let mut state = state_in_cell_edit("abc", 1);

            reduce(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::Right),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 2);
        }

        #[test]
        fn move_cursor_home_jumps_to_start() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::Home),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 0);
        }

        #[test]
        fn move_cursor_end_jumps_to_end() {
            let mut state = state_in_cell_edit("abc", 0);

            reduce(
                &mut state,
                &Action::ResultCellEditMoveCursor(CursorMove::End),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.input.cursor(), 3);
        }

        #[test]
        fn input_inserts_at_cursor_not_at_end() {
            let mut state = state_in_cell_edit("ac", 1);

            reduce(
                &mut state,
                &Action::ResultCellEditInput('b'),
                Instant::now(),
            );

            assert_eq!(state.cell_edit.draft_value(), "abc");
            assert_eq!(state.cell_edit.input.cursor(), 2);
        }

        #[test]
        fn backspace_removes_char_before_cursor() {
            let mut state = state_in_cell_edit("abc", 2);

            reduce(&mut state, &Action::ResultCellEditBackspace, Instant::now());

            assert_eq!(state.cell_edit.draft_value(), "ac");
            assert_eq!(state.cell_edit.input.cursor(), 1);
        }
    }
}
