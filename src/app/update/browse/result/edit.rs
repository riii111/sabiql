use std::time::Instant;

use crate::cmd::effect::Effect;
#[cfg(test)]
use crate::domain::ColumnAttributes;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;

use crate::policy::preview_cell_text::{preview_cell_text_diff_handling, uses_jsonb_detail_modal};
use crate::policy::write::inline_cell_edit::text_for_inline_edit;
use crate::update::helpers::{EditGuardrailError, editable_preview_base, ensure_column_writable};

fn cell_uses_jsonb_detail_modal(state: &AppState) -> bool {
    let Some(col_idx) = state.result_interaction.selection().cell() else {
        return false;
    };
    let Some(td) = state.session.table_detail() else {
        return false;
    };
    if !state.query.pagination.matches_table(td) {
        return false;
    }
    let Some(column) = td.columns.get(col_idx) else {
        return false;
    };
    let handling = preview_cell_text_diff_handling(
        state.session.active_database_type_or_default(),
        column.data_type.as_str(),
    );
    uses_jsonb_detail_modal(handling)
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

    let (result, identity) = editable_preview_base(state)?;

    let column_name = result
        .columns
        .get(col_idx)
        .ok_or(EditGuardrailError::ColumnIndexOutOfBounds)?;
    ensure_column_writable(state, column_name, &identity)?;

    if row_idx >= result.values().len() {
        return Err(EditGuardrailError::RowIndexOutOfBounds);
    }
    if identity.identity_pairs_for_row(result, row_idx).is_none() {
        return Err(EditGuardrailError::StableKeyColumnsMissing);
    }

    let cell_value = text_for_inline_edit(
        state.session.active_database_type_or_default(),
        result
            .value_at(row_idx, col_idx)
            .ok_or(EditGuardrailError::CellIndexOutOfBounds)?,
    )?;

    Ok((row_idx, col_idx, cell_value))
}

pub fn reduce_edit(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::ResultEnterCellEdit => {
            if state.session.is_read_only() {
                state
                    .messages
                    .set_error_at("Read-only mode: editing is disabled".to_string(), now);
                return DispatchResult::handled();
            }

            // JSONB columns open the dedicated detail modal instead of inline edit
            if cell_uses_jsonb_detail_modal(state) {
                return DispatchResult::handled_with(vec![Effect::DispatchActions(vec![
                    Action::OpenModal(ModalKind::JsonbDetail),
                ])]);
            }

            match editable_cell_context(state) {
                Ok((row_idx, col_idx, value)) => {
                    state
                        .result_interaction
                        .ensure_cell_edit_at(row_idx, col_idx, value);
                    state.modal.set_mode(InputMode::CellEdit);
                    DispatchResult::handled()
                }
                Err(reason) => {
                    state.messages.set_error_at(reason.to_string(), now);
                    DispatchResult::handled()
                }
            }
        }
        Action::ResultCancelCellEdit => {
            state.result_interaction.leave_cell_edit();
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }
        Action::ResultDiscardCellEdit => {
            state.result_interaction.discard_cell_edit();
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }
        Action::TextInput {
            target: InputTarget::ResultCellEdit,
            ch: c,
        } => {
            state.result_interaction.cell_edit_insert_char(*c);
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::ResultCellEdit,
        } => {
            state.result_interaction.cell_edit_backspace();
            DispatchResult::handled()
        }
        Action::TextDelete {
            target: InputTarget::ResultCellEdit,
        } => {
            state.result_interaction.cell_edit_delete();
            DispatchResult::handled()
        }
        Action::TextKill {
            target: InputTarget::ResultCellEdit,
            direction,
        } => {
            let killed = state.result_interaction.cell_edit_kill(*direction);
            state.record_kill(killed);
            DispatchResult::handled()
        }
        Action::TextYank {
            target: InputTarget::ResultCellEdit,
        } => {
            if let Some(killed) = state.kill_buffer().map(str::to_owned) {
                state.result_interaction.cell_edit_yank(&killed);
            }
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: InputTarget::ResultCellEdit,
            direction: m,
        } => {
            state.result_interaction.cell_edit_move_cursor(*m);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub use crate::domain::Column;
    use crate::domain::connection::ConnectionId;
    use crate::domain::{DatabaseType, QueryResult, QuerySource, QueryValue, Table};
    use crate::update::action::{CursorMove, TextKillDirection};
    use rstest::rstest;
    use std::sync::Arc;

    mod cell_edit_entry_guardrails {
        use crate::test_support;

        use super::*;

        pub(super) fn minimal_users_table() -> Table {
            Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                primary_key: Some(vec!["id".to_string()]),
                ..test_support::table::minimal("", "")
            }
        }

        pub(super) fn preview_state_with_selection() -> AppState {
            let mut state = AppState::new("test".to_string());
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    vec!["id".to_string(), "name".to_string()],
                    vec![vec!["1".to_string(), "alice".to_string()]],
                    1,
                    QuerySource::Preview,
                )));
            state.query.pagination.reset_for_table("public", "users");
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
                .replace_cell_edit_draft("modified".to_string());
            state.modal.set_mode(InputMode::Normal);

            reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

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
                .replace_cell_edit_draft("stale-modified".to_string());

            reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert_eq!(state.result_interaction.cell_edit().col(), Some(1));
            assert_eq!(state.result_interaction.cell_edit().draft_value(), "alice");
        }

        #[rstest]
        #[case(QueryValue::Null, "NULL cells are not editable inline yet")]
        #[case(QueryValue::Blob(vec![0, 255]), "BLOB cells are not editable inline")]
        fn unsupported_cell_blocks_cell_edit_entry(
            #[case] cell_value: QueryValue,
            #[case] expected_error: &str,
        ) {
            let mut state = AppState::new("test".to_string());
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    String::new(),
                    vec!["id".to_string(), "payload".to_string()],
                    vec![vec![QueryValue::text("1"), cell_value]],
                    1,
                    QuerySource::Preview,
                )));
            state.query.pagination.reset_for_table("public", "users");
            state.result_interaction.activate_cell(0, 1);
            state
                .session
                .set_table_detail_raw(Some(minimal_users_table()));

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(state.messages.last_error(), Some(expected_error));
        }

        #[test]
        fn sqlite_numeric_literal_enters_inline_edit() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    String::new(),
                    vec!["id".to_string(), "payload".to_string()],
                    vec![vec![
                        QueryValue::SqlLiteral("1".to_string()),
                        QueryValue::SqlLiteral("42".to_string()),
                    ]],
                    1,
                    QuerySource::Preview,
                )));
            state.query.pagination.reset_for_table("public", "users");
            state.result_interaction.activate_cell(0, 1);
            state
                .session
                .set_table_detail_raw(Some(minimal_users_table()));

            reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert_eq!(state.result_interaction.cell_edit().draft_value(), "42");
        }

        #[test]
        fn text_with_nul_keeps_raw_draft_on_entry() {
            let mut state = AppState::new("test".to_string());
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    String::new(),
                    vec!["id".to_string(), "payload".to_string()],
                    vec![vec![QueryValue::text("1"), QueryValue::text("a\0b")]],
                    1,
                    QuerySource::Preview,
                )));
            state.query.pagination.reset_for_table("public", "users");
            state.result_interaction.activate_cell(0, 1);
            state
                .session
                .set_table_detail_raw(Some(minimal_users_table()));

            reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert_eq!(
                state.result_interaction.cell_edit().original_value(),
                "a\0b"
            );
            assert_eq!(state.result_interaction.cell_edit().draft_value(), "a\0b");
        }

        #[test]
        fn stale_table_detail_blocks_cell_edit_entry() {
            let mut state = preview_state_with_selection();
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "posts".to_string(),
                primary_key: Some(vec!["id".to_string()]),
                ..test_support::table::minimal("", "")
            }));

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(
                state.messages.last_error(),
                Some("Table metadata does not match current preview target")
            );
        }

        #[test]
        fn view_detail_blocks_cell_edit_entry() {
            let mut state = preview_state_with_selection();
            let mut table = minimal_users_table();
            table.kind_info = test_support::table::view_kind_info();
            state.session.set_table_detail_raw(Some(table));

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(
                state.messages.last_error(),
                Some("Preview target is read-only: view")
            );
        }

        #[test]
        fn read_only_column_blocks_cell_edit_entry() {
            let mut state = preview_state_with_selection();
            let mut table = minimal_users_table();
            table.columns = vec![
                Column {
                    attributes: ColumnAttributes::PRIMARY_KEY,
                    ..test_support::column::test_nullable_column("id", "integer", 1)
                },
                Column {
                    attributes: ColumnAttributes::READ_ONLY | ColumnAttributes::GENERATED,
                    ..test_support::column::test_nullable_column("name", "text", 2)
                },
            ];
            state.session.set_table_detail_raw(Some(table));

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert_eq!(
                state.messages.last_error(),
                Some("Read-only column cannot be edited: name (generated)")
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

            reduce_edit(&mut state, &Action::ResultCancelCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

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
                .replace_cell_edit_draft("bob".to_string());
            state.modal.set_mode(InputMode::CellEdit);

            reduce_edit(&mut state, &Action::ResultCancelCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.result_interaction.cell_edit().is_active());
            assert_eq!(state.result_interaction.cell_edit().draft_value(), "bob");
        }
    }

    mod jsonb_dispatch {
        use crate::test_support;

        use super::*;

        fn state_with_jsonb_column() -> AppState {
            let mut state = cell_edit_entry_guardrails::preview_state_with_selection();
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "database",
                DatabaseType::PostgreSQL,
                "postgres://localhost/test",
            );
            let mut table = cell_edit_entry_guardrails::minimal_users_table();
            table.columns = vec![
                Column {
                    attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                    ..test_support::column::test_nullable_column("id", "integer", 1)
                },
                test_support::column::test_nullable_column("name", "jsonb", 2),
            ];
            state.session.set_table_detail_raw(Some(table));
            state
        }

        #[test]
        fn jsonb_cell_returns_dispatch_to_open_jsonb_detail() {
            let mut state = state_with_jsonb_column();

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::DispatchActions(actions) if matches!(actions.as_slice(), [Action::OpenModal(ModalKind::JsonbDetail)])
            ));
        }

        #[test]
        fn sqlite_jsonb_cell_opens_inline_edit() {
            let mut state = state_with_jsonb_column();
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("sqlite-test"),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert!(state.result_interaction.cell_edit().is_active());
        }

        #[test]
        fn sqlite_text_json_cell_edits_raw_value() {
            let mut state = cell_edit_entry_guardrails::preview_state_with_selection();
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("sqlite-test"),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    vec!["id".to_string(), "name".to_string()],
                    vec![vec!["1".to_string(), r#"{"b":2,"a":1}"#.to_string()]],
                    1,
                    QuerySource::Preview,
                )));
            state
                .session
                .set_table_detail_raw(Some(cell_edit_entry_guardrails::minimal_users_table()));

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::CellEdit);
            assert_eq!(
                state.result_interaction.cell_edit().draft_value(),
                r#"{"b":2,"a":1}"#
            );
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
            state.session.enable_read_only();

            let effects = reduce_edit(&mut state, &Action::ResultEnterCellEdit, Instant::now())
                .into_effects()
                .expect("reducer should handle action");

            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(state.messages.last_error().is_some());
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
            state.result_interaction.cell_edit_set_cursor(cursor);
            state
        }

        #[test]
        fn delete_removes_char_at_cursor() {
            let mut state = state_in_cell_edit("abcd", 1);

            reduce_edit(
                &mut state,
                &Action::TextDelete {
                    target: InputTarget::ResultCellEdit,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().draft_value(), "acd");
            assert_eq!(state.result_interaction.cell_edit().input().cursor(), 1);
        }

        #[test]
        fn delete_at_end_is_noop() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce_edit(
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

            reduce_edit(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Left,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input().cursor(), 1);
        }

        #[test]
        fn move_cursor_right_increments() {
            let mut state = state_in_cell_edit("abc", 1);

            reduce_edit(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Right,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input().cursor(), 2);
        }

        #[test]
        fn move_cursor_home_jumps_to_start() {
            let mut state = state_in_cell_edit("abc", 3);

            reduce_edit(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::Home,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input().cursor(), 0);
        }

        #[test]
        fn move_cursor_end_jumps_to_end() {
            let mut state = state_in_cell_edit("abc", 0);

            reduce_edit(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::ResultCellEdit,
                    direction: CursorMove::End,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().input().cursor(), 3);
        }

        #[test]
        fn input_inserts_at_cursor_not_at_end() {
            let mut state = state_in_cell_edit("ac", 1);

            reduce_edit(
                &mut state,
                &Action::TextInput {
                    target: InputTarget::ResultCellEdit,
                    ch: 'b',
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().draft_value(), "abc");
            assert_eq!(state.result_interaction.cell_edit().input().cursor(), 2);
        }

        #[test]
        fn backspace_removes_char_before_cursor() {
            let mut state = state_in_cell_edit("abc", 2);

            reduce_edit(
                &mut state,
                &Action::TextBackspace {
                    target: InputTarget::ResultCellEdit,
                },
                Instant::now(),
            );

            assert_eq!(state.result_interaction.cell_edit().draft_value(), "ac");
            assert_eq!(state.result_interaction.cell_edit().input().cursor(), 1);
        }

        #[test]
        fn kill_then_yank_restores_cell_edit_text() {
            let mut state = state_in_cell_edit("before after", 7);

            reduce_edit(
                &mut state,
                &Action::TextKill {
                    target: InputTarget::ResultCellEdit,
                    direction: TextKillDirection::ToLineEnd,
                },
                Instant::now(),
            );
            reduce_edit(
                &mut state,
                &Action::TextYank {
                    target: InputTarget::ResultCellEdit,
                },
                Instant::now(),
            );

            assert_eq!(
                state.result_interaction.cell_edit().draft_value(),
                "before after"
            );
            assert_eq!(state.kill_buffer(), Some("after"));
        }
    }
}
