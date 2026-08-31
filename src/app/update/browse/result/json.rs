use crate::cmd::effect::Effect;
use crate::domain::QueryValue;
use crate::model::app_state::AppState;
use crate::model::browse::json_detail::{JsonDetailMode, JsonDetailState};
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::shared::ui_state::DEFAULT_JSON_DETAIL_EDITOR_VISIBLE_ROWS;
use crate::policy::preview_cell_text::CellPresentationPolicy;
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::{
    EditGuardrailError, clipboard_unavailable, editable_preview_base, ensure_column_writable,
    find_text_matches,
};
use std::time::Instant;

pub fn reduce_json(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::JsonDetail) => {
            let Some(result) = state.query.visible_result().filter(|r| !r.is_error()) else {
                return DispatchResult::handled();
            };

            let Some(table_detail) = state.session.table_detail() else {
                return DispatchResult::handled();
            };
            if table_detail.schema != state.query.pagination.schema()
                || table_detail.name != state.query.pagination.table()
            {
                return DispatchResult::handled();
            }

            let Some(row_idx) = state.result_interaction.selection().row() else {
                return DispatchResult::handled();
            };
            let Some(col_idx) = state.result_interaction.selection().cell() else {
                return DispatchResult::handled();
            };

            let database_type = state.session.active_database_type_or_default();
            let Some(column) = state.visible_preview_column(col_idx) else {
                return DispatchResult::handled();
            };
            let policy = CellPresentationPolicy::new(database_type, column.data_type.as_str(), "");
            if !policy.uses_json_detail_modal() {
                return DispatchResult::handled();
            }

            let cell_value = if result.has_typed_values() {
                let Some(value) = result.value_at(row_idx, col_idx) else {
                    return DispatchResult::handled();
                };
                if matches!(value, QueryValue::Null) {
                    return DispatchResult::handled();
                }
                value.display_value()
            } else {
                match result.display_value_at(row_idx, col_idx) {
                    Some(value) if !value.is_empty() => value,
                    _ => return DispatchResult::handled(),
                }
            };

            let pretty_original = match serde_json::from_str::<serde_json::Value>(&cell_value) {
                Ok(value) => {
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| cell_value.clone())
                }
                Err(err) => {
                    state.messages.set_error(format!("Invalid JSON: {err}"));
                    return DispatchResult::handled();
                }
            };

            state.json_detail = JsonDetailState::open_pretty(
                row_idx,
                col_idx,
                column.name.clone(),
                cell_value,
                pretty_original,
            );
            state.modal.push_mode(InputMode::JsonDetail);
            DispatchResult::handled()
        }

        Action::CloseModal(ModalKind::JsonDetail) => {
            apply_pending_edit_as_draft(state);
            state.json_detail.close();
            state.modal.pop_mode();
            DispatchResult::handled()
        }

        Action::JsonYankAll => {
            let json = state.json_detail.current_json_for_yank();
            DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                content: json,
                on_success: Some(Box::new(Action::JsonYankSuccess)),
                on_failure: Some(Box::new(clipboard_unavailable())),
            }])
        }

        Action::JsonYankSuccess => {
            state.flash_timers.set(FlashId::JsonDetail, now);
            DispatchResult::handled()
        }

        Action::JsonEnterEdit => {
            if state.session.is_read_only() {
                state
                    .messages
                    .set_error("Read-only mode: editing is disabled".to_string());
                return DispatchResult::handled();
            }
            if let Err(reason) = ensure_json_column_writable(state) {
                state.messages.set_error(reason.to_string());
                return DispatchResult::handled();
            }
            state.json_detail.enter_edit();
            state.modal.replace_mode(InputMode::JsonEdit);
            DispatchResult::handled()
        }

        Action::JsonAppendInsert => {
            if state.session.is_read_only() {
                state
                    .messages
                    .set_error("Read-only mode: editing is disabled".to_string());
                return DispatchResult::handled();
            }
            if let Err(reason) = ensure_json_column_writable(state) {
                state.messages.set_error(reason.to_string());
                return DispatchResult::handled();
            }
            state
                .json_detail
                .editor_mut()
                .move_cursor(CursorMove::LineEnd);
            update_editor_scroll(state);
            state.json_detail.enter_edit();
            state.modal.replace_mode(InputMode::JsonEdit);
            DispatchResult::handled()
        }

        Action::JsonExitEdit => {
            state.json_detail.exit_edit();
            state.modal.replace_mode(InputMode::JsonDetail);
            DispatchResult::handled()
        }

        Action::TextInput {
            target: InputTarget::JsonEdit,
            ch,
        } => {
            if *ch == '\n' {
                state.json_detail.editor_mut().insert_newline();
            } else if *ch == '\t' {
                state.json_detail.editor_mut().insert_tab();
            } else {
                state.json_detail.editor_mut().insert_char(*ch);
            }
            update_editor_scroll(state);
            state.json_detail.validate_editor_content();
            DispatchResult::handled()
        }

        Action::TextBackspace {
            target: InputTarget::JsonEdit,
        } => {
            state.json_detail.editor_mut().backspace();
            update_editor_scroll(state);
            state.json_detail.validate_editor_content();
            DispatchResult::handled()
        }

        Action::TextDelete {
            target: InputTarget::JsonEdit,
        } => {
            state.json_detail.editor_mut().delete();
            update_editor_scroll(state);
            state.json_detail.validate_editor_content();
            DispatchResult::handled()
        }
        Action::TextKill {
            target: InputTarget::JsonEdit,
            direction,
        } => {
            let killed = state.json_detail.editor_mut().kill(*direction);
            state.record_kill(killed);
            update_editor_scroll(state);
            state.json_detail.validate_editor_content();
            DispatchResult::handled()
        }
        Action::TextYank {
            target: InputTarget::JsonEdit,
        } => {
            if let Some(killed) = state.kill_buffer().map(str::to_owned) {
                state.json_detail.editor_mut().yank(&killed);
                update_editor_scroll(state);
                state.json_detail.validate_editor_content();
            }
            DispatchResult::handled()
        }

        Action::TextMoveCursor {
            target: InputTarget::JsonEdit,
            direction,
        } => {
            match direction {
                CursorMove::ViewportTop
                | CursorMove::ViewportMiddle
                | CursorMove::ViewportBottom => {
                    let visible_rows = effective_visible_rows(state);
                    state
                        .json_detail
                        .editor_mut()
                        .move_cursor_to_viewport_position(*direction, visible_rows);
                }
                _ => state.json_detail.editor_mut().move_cursor(*direction),
            }
            update_editor_scroll(state);
            state.ui.set_key_sequence(KeySequenceState::Idle);
            DispatchResult::handled()
        }

        Action::Paste(text) if state.input_mode() == InputMode::JsonEdit => {
            state.json_detail.editor_mut().insert_str(text);
            update_editor_scroll(state);
            state.json_detail.validate_editor_content();
            DispatchResult::handled()
        }

        Action::JsonEnterSearch => {
            state.json_detail.enter_search();
            DispatchResult::handled()
        }

        Action::JsonExitSearch => {
            state.json_detail.exit_search();
            DispatchResult::handled()
        }

        Action::JsonSearchSubmit => {
            state.json_detail.exit_search();
            jump_to_current_match(state);
            DispatchResult::handled()
        }

        Action::JsonSearchNext => {
            state.json_detail.search_mut().advance_to_next_match();
            jump_to_current_match(state);
            DispatchResult::handled()
        }

        Action::JsonSearchPrev => {
            state.json_detail.search_mut().advance_to_prev_match();
            jump_to_current_match(state);
            DispatchResult::handled()
        }

        Action::TextInput {
            target: InputTarget::JsonSearch,
            ch,
        } => {
            state.json_detail.search_mut().input_mut().insert_char(*ch);
            update_search_matches(state);
            DispatchResult::handled()
        }

        Action::TextBackspace {
            target: InputTarget::JsonSearch,
        } => {
            state.json_detail.search_mut().input_mut().backspace();
            update_search_matches(state);
            DispatchResult::handled()
        }

        Action::TextDelete {
            target: InputTarget::JsonSearch,
        } => {
            state.json_detail.search_mut().input_mut().delete();
            update_search_matches(state);
            DispatchResult::handled()
        }
        Action::TextKill {
            target: InputTarget::JsonSearch,
            direction,
        } => {
            let killed = state.json_detail.search_mut().input_mut().kill(*direction);
            state.record_kill(killed);
            update_search_matches(state);
            DispatchResult::handled()
        }
        Action::TextYank {
            target: InputTarget::JsonSearch,
        } => {
            if let Some(killed) = state.kill_buffer().map(str::to_owned) {
                state.json_detail.search_mut().input_mut().yank(&killed);
                update_search_matches(state);
            }
            DispatchResult::handled()
        }

        Action::Paste(text)
            if state.input_mode() == InputMode::JsonDetail
                && state.json_detail.mode() == JsonDetailMode::Searching =>
        {
            let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            state
                .json_detail
                .search_mut()
                .input_mut()
                .insert_str(&clean);
            update_search_matches(state);
            DispatchResult::handled()
        }

        Action::TextMoveCursor {
            target: InputTarget::JsonSearch,
            direction,
        } => {
            state
                .json_detail
                .search_mut()
                .input_mut()
                .move_cursor(*direction);
            DispatchResult::handled()
        }

        _ => DispatchResult::pass(),
    }
}

fn ensure_json_column_writable(state: &AppState) -> Result<(), EditGuardrailError> {
    let (_, identity) = editable_preview_base(state)?;
    ensure_column_writable(state, state.json_detail.column_name(), &identity)
}

fn update_search_matches(state: &mut AppState) {
    let query = state.json_detail.search().input().content().to_string();
    let matches = find_text_matches(state.json_detail.editor().content(), &query);
    state.json_detail.search_mut().set_matches(matches);
}

fn jump_to_current_match(state: &mut AppState) {
    let search = state.json_detail.search();
    if let Some(&match_pos) = search.matches().get(search.current_match()) {
        state.json_detail.editor_mut().set_cursor(match_pos);
        update_editor_scroll(state);
    }
}

fn update_editor_scroll(state: &mut AppState) {
    let visible_rows = effective_visible_rows(state);
    state.json_detail.editor_mut().update_scroll(visible_rows);
}

fn effective_visible_rows(state: &AppState) -> usize {
    match state.ui.json_detail_editor_visible_rows() {
        0 => DEFAULT_JSON_DETAIL_EDITOR_VISIBLE_ROWS,
        rows => rows,
    }
}

fn apply_pending_edit_as_draft(state: &mut AppState) {
    if !state.json_detail.has_pending_changes() {
        return;
    }

    let content = state.json_detail.editor().content().to_string();

    let compact = serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|value| serde_json::to_string(&value).ok())
        .unwrap_or(content);
    let row = state.json_detail.row();
    let col = state.json_detail.col();
    let original_cell = state
        .query
        .visible_result()
        .and_then(|result| result.display_value_at(row, col))
        .unwrap_or_default();
    state
        .result_interaction
        .begin_cell_edit(row, col, original_cell);
    state.result_interaction.clear_write_preview();
    state.result_interaction.replace_cell_edit_draft(compact);
}

#[cfg(test)]
mod tests {
    use crate::domain::Column;
    use crate::domain::ColumnAttributes;
    use crate::test_support;
    use crate::update::test_fixtures;

    use super::*;
    use crate::domain::connection::ConnectionId;
    use crate::domain::{DatabaseType, QueryResult, QuerySource, Table};
    use crate::services::AppServices;
    use crate::update::action::TextKillDirection;
    use std::sync::Arc;

    fn json_table() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            columns: vec![
                Column {
                    attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                    ..test_support::column::test_nullable_column("id", "integer", 1)
                },
                test_support::column::test_nullable_column("settings", "jsonb", 2),
            ],
            primary_key: Some(vec!["id".to_string()]),
            ..test_support::table::minimal("", "")
        }
    }

    fn state_with_json_cell() -> AppState {
        state_with_json_value(r#"{"theme":"dark","count":5}"#)
    }

    fn state_with_json_value(cell_value: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                String::new(),
                vec!["id".to_string(), "settings".to_string()],
                vec![vec!["1".to_string(), cell_value.to_string()]],
                1,
                QuerySource::Preview,
            )));
        state.query.pagination.reset_for_table("public", "users");
        state.session.set_table_detail_raw(Some(json_table()));
        state.result_interaction.activate_cell(0, 1);
        state
    }

    fn state_with_mysql_json_value(cell_value: &str) -> AppState {
        let mut state = state_with_json_value(cell_value);
        test_fixtures::activate_mysql_connection(&mut state, "mysql://localhost/test");
        let mut table = json_table();
        table.columns[1].data_type = "json".to_string();
        state.session.set_table_detail_raw(Some(table));
        state
    }

    fn state_with_hidden_primary_key_json_value() -> AppState {
        let mut state = state_with_json_value(r#"{"theme":"dark"}"#);
        state.query.set_current_result(Arc::new(
            QueryResult::success(
                String::new(),
                vec!["settings".to_string()],
                vec![vec![r#"{"theme":"dark"}"#.to_string()]],
                1,
                QuerySource::Preview,
            )
            .with_explicit_row_identity(vec!["id".to_string()], vec![vec![QueryValue::text("1")]]),
        ));
        state.query.pagination.reset_for_table("public", "users");
        state.session.set_table_detail_raw(Some(Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            columns: vec![
                Column {
                    attributes: ColumnAttributes::PRIMARY_KEY
                        | ColumnAttributes::HIDDEN
                        | ColumnAttributes::READ_ONLY,
                    ..test_support::column::test_nullable_column("id", "integer", 1)
                },
                test_support::column::test_nullable_column("settings", "jsonb", 2),
            ],
            primary_key: Some(vec!["id".to_string()]),
            ..test_support::table::minimal("", "")
        }));
        state.result_interaction.activate_cell(0, 0);
        state
    }

    fn open_detail(state: &mut AppState) {
        reduce_json(
            state,
            &Action::OpenModal(ModalKind::JsonDetail),
            Instant::now(),
        );
    }

    fn cursor_position(content: &str, cursor: usize) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;

        for (idx, ch) in content.chars().enumerate() {
            if idx >= cursor {
                break;
            }
            if ch == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        (row, col)
    }

    mod entry_guards {
        use super::*;
        use rstest::rstest;

        #[test]
        fn opens_on_valid_json_cell() {
            let mut state = state_with_json_cell();

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(state.json_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::JsonDetail);
        }

        #[rstest]
        #[case(r#"{"key":"value"}"#)]
        #[case(r"[1,2,3]")]
        #[case("42")]
        #[case("null")]
        #[case(r#""null""#)]
        fn opens_on_mysql_json_documents(#[case] cell_value: &str) {
            let mut state = state_with_mysql_json_value(cell_value);

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(state.json_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::JsonDetail);
            assert_eq!(state.json_detail.original_json(), cell_value);
        }

        #[test]
        fn mysql_json_sql_null_does_not_open_detail() {
            let mut state = state_with_mysql_json_value("");
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    String::new(),
                    vec!["id".to_string(), "settings".to_string()],
                    vec![vec![QueryValue::text("1"), QueryValue::Null]],
                    1,
                    QuerySource::Preview,
                )));

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
            assert_eq!(state.messages.last_error(), None);
        }

        #[test]
        fn blocked_on_non_json_column() {
            let mut state = state_with_json_cell();
            state.result_interaction.move_cell(0);

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn blocked_on_sqlite_json_column() {
            let mut state = state_with_json_cell();
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("sqlite-test"),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn blocked_on_null_cell() {
            let mut state = state_with_json_cell();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    vec!["id".to_string(), "settings".to_string()],
                    vec![vec!["1".to_string(), String::new()]],
                    1,
                    QuerySource::Preview,
                )));

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
        }

        #[test]
        fn blocked_on_typed_null_cell() {
            let mut state = state_with_json_cell();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    String::new(),
                    vec!["id".to_string(), "settings".to_string()],
                    vec![vec![QueryValue::text("1"), QueryValue::Null]],
                    1,
                    QuerySource::Preview,
                )));

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
            assert_eq!(state.messages.last_error, None);
        }

        #[test]
        fn blocked_on_adhoc_result() {
            let mut state = state_with_json_cell();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    vec!["id".to_string(), "settings".to_string()],
                    vec![vec!["1".to_string(), r#"{"theme":"dark"}"#.to_string()]],
                    1,
                    QuerySource::Adhoc,
                )));

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
        }

        #[test]
        fn blocked_without_table_detail() {
            let mut state = state_with_json_cell();
            state.session.set_table_detail_raw(None);

            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
        }
    }

    mod navigation {
        use super::*;

        #[test]
        fn close_clears_state() {
            let mut state = state_with_json_cell();
            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );
            assert!(state.json_detail.is_active());

            reduce_json(
                &mut state,
                &Action::CloseModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert!(!state.json_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }
    }

    mod edit_lifecycle {
        use super::*;
        use crate::model::shared::key_sequence::Prefix;
        use crate::update::action::CursorMove;
        use crate::update::reducer::reduce;
        use rstest::rstest;

        #[test]
        fn enter_edit_switches_to_json_edit_mode() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);

            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonEdit);
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Editing);
        }

        #[test]
        fn mysql_json_detail_starts_edit_mode() {
            let mut state = state_with_mysql_json_value(r#"{"key":"value"}"#);
            let services = AppServices::stub();
            let now = Instant::now();

            reduce(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce(&mut state, Action::JsonEnterEdit, now, &services);

            assert_eq!(state.input_mode(), InputMode::JsonEdit);
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Editing);
        }

        #[test]
        fn hidden_primary_key_does_not_shift_json_detail_column() {
            let mut state = state_with_hidden_primary_key_json_value();

            open_detail(&mut state);

            assert!(state.json_detail.is_active());
            assert_eq!(state.json_detail.column_name(), "settings");
        }

        #[test]
        fn enter_edit_preserves_cursor_from_normal_mode() {
            let mut state = state_with_json_value(r#"{"items":["admin","writer"]}"#);
            open_detail(&mut state);
            reduce_json(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );
            reduce_json(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Right,
                },
                Instant::now(),
            );
            let expected = state.json_detail.editor().cursor();

            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());

            assert_eq!(state.json_detail.editor().cursor(), expected);
        }

        #[test]
        fn append_insert_moves_to_current_line_end_before_editing() {
            let mut state = state_with_json_value(r#"{"items":["admin","writer"]}"#);
            open_detail(&mut state);
            state
                .json_detail
                .editor_mut()
                .set_content_with_cursor("abc\ndef".to_string(), 1);

            reduce_json(&mut state, &Action::JsonAppendInsert, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonEdit);
            assert_eq!(state.json_detail.editor().cursor_to_position(), (0, 3));
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Editing);
        }

        #[test]
        fn enter_edit_blocks_read_only_column() {
            let mut state = state_with_json_cell();
            state.session.set_table_detail_raw(Some(Table {
                columns: vec![
                    Column {
                        attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                        ..test_support::column::test_nullable_column("id", "integer", 1)
                    },
                    Column {
                        attributes: ColumnAttributes::READ_ONLY | ColumnAttributes::GENERATED,
                        ..test_support::column::test_nullable_column("settings", "jsonb", 2)
                    },
                ],
                ..json_table()
            }));
            open_detail(&mut state);

            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonDetail);
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Viewing);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Read-only column cannot be edited: settings (generated)")
            );
        }

        #[test]
        fn append_insert_blocks_read_only_column() {
            let mut state = state_with_json_cell();
            state.session.set_table_detail_raw(Some(Table {
                columns: vec![
                    Column {
                        attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                        ..test_support::column::test_nullable_column("id", "integer", 1)
                    },
                    Column {
                        attributes: ColumnAttributes::READ_ONLY | ColumnAttributes::GENERATED,
                        ..test_support::column::test_nullable_column("settings", "jsonb", 2)
                    },
                ],
                ..json_table()
            }));
            open_detail(&mut state);

            reduce_json(&mut state, &Action::JsonAppendInsert, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonDetail);
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Viewing);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Read-only column cannot be edited: settings (generated)")
            );
        }

        #[test]
        fn movement_updates_scroll_with_current_editor_viewport_height() {
            let mut state = state_with_json_value(r#"{"items":["admin","writer","reader"]}"#);
            state.ui.set_json_detail_editor_visible_rows(2);
            open_detail(&mut state);

            reduce_json(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );
            reduce_json(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );

            assert_eq!(state.json_detail.editor().scroll_row(), 1);
        }

        #[rstest]
        #[case(CursorMove::ViewportTop, 0)]
        #[case(CursorMove::ViewportMiddle, 1)]
        #[case(CursorMove::ViewportBottom, 2)]
        fn viewport_cursor_moves_follow_visible_rows(
            #[case] movement: CursorMove,
            #[case] expected_row: usize,
        ) {
            let mut state =
                state_with_json_value("{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3,\n  \"d\": 4\n}");
            state.ui.set_json_detail_editor_visible_rows(3);
            open_detail(&mut state);
            state.modal.replace_mode(InputMode::JsonEdit);
            state.json_detail.set_mode(JsonDetailMode::Editing);
            state
                .json_detail
                .editor_mut()
                .set_content_with_cursor("line1\nline2\nline3\nline4".to_string(), 0);

            reduce_json(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: movement,
                },
                Instant::now(),
            );

            assert_eq!(
                state.json_detail.editor().cursor_to_position().0,
                expected_row
            );
        }

        #[test]
        fn cursor_movement_clears_pending_key_sequence() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            state.modal.replace_mode(InputMode::JsonEdit);
            state.json_detail.set_mode(JsonDetailMode::Editing);
            state
                .ui
                .set_key_sequence(KeySequenceState::WaitingSecondKey(Prefix::G));

            reduce_json(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Right,
                },
                Instant::now(),
            );

            assert_eq!(state.ui.key_sequence().pending_prefix(), None);
        }

        #[test]
        fn enter_edit_blocked_in_read_only_mode() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            state.session.enable_read_only();

            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonDetail);
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn append_insert_blocked_in_read_only_mode() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            state.session.enable_read_only();
            let cursor_before = state.json_detail.editor().cursor();

            reduce_json(&mut state, &Action::JsonAppendInsert, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonDetail);
            assert_eq!(state.json_detail.editor().cursor(), cursor_before);
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn exit_edit_returns_to_viewing_mode() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());

            reduce_json(&mut state, &Action::JsonExitEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonDetail);
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Viewing);
            assert!(state.json_detail.is_active());
        }

        #[test]
        fn kill_then_yank_restores_json_editor_text() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());
            state
                .json_detail
                .editor_mut()
                .set_content_with_cursor("before after".to_string(), 7);

            reduce_json(
                &mut state,
                &Action::TextKill {
                    target: InputTarget::JsonEdit,
                    direction: TextKillDirection::ToLineEnd,
                },
                Instant::now(),
            );
            reduce_json(
                &mut state,
                &Action::TextYank {
                    target: InputTarget::JsonEdit,
                },
                Instant::now(),
            );

            assert_eq!(state.json_detail.editor().content(), "before after");
            assert_eq!(state.kill_buffer(), Some("after"));
        }

        #[test]
        fn reenter_edit_with_pending_changes_preserves_existing_cursor() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());
            state
                .json_detail
                .editor_mut()
                .set_content_with_cursor(r#"{"theme":"light","count":5}"#.to_string(), 7);
            reduce_json(&mut state, &Action::JsonExitEdit, Instant::now());

            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());

            assert_eq!(state.json_detail.editor().cursor(), 7);
        }

        #[test]
        fn close_after_edit_with_valid_changes_stores_draft() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());
            state
                .json_detail
                .editor_mut()
                .set_content(r#"{"theme":"light","count":5}"#.to_string());
            reduce_json(&mut state, &Action::JsonExitEdit, Instant::now());

            reduce_json(
                &mut state,
                &Action::CloseModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.json_detail.is_active());
            assert!(state.result_interaction.cell_edit().has_pending_draft());
        }

        #[test]
        fn close_after_edit_without_changes_no_draft() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterEdit, Instant::now());
            reduce_json(&mut state, &Action::JsonExitEdit, Instant::now());

            reduce_json(
                &mut state,
                &Action::CloseModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.result_interaction.cell_edit().has_pending_draft());
        }
    }

    mod yank {
        use super::*;

        #[test]
        fn copies_all_text_to_clipboard() {
            let mut state = state_with_json_cell();
            reduce_json(
                &mut state,
                &Action::OpenModal(ModalKind::JsonDetail),
                Instant::now(),
            );

            let now = Instant::now();
            let effects = reduce_json(&mut state, &Action::JsonYankAll, now);

            let effects = effects.into_effects().expect("should return effects");
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard {
                    content,
                    on_success,
                    ..
                } => {
                    assert!(content.contains("theme"));
                    assert!(matches!(
                        on_success.as_deref(),
                        Some(Action::JsonYankSuccess)
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
            assert!(!state.flash_timers.is_active(FlashId::JsonDetail, now));
        }

        #[test]
        fn success_sets_flash() {
            let mut state = state_with_json_cell();
            let now = Instant::now();

            reduce_json(&mut state, &Action::JsonYankSuccess, now);

            assert!(state.flash_timers.is_active(FlashId::JsonDetail, now));
        }
    }

    mod search {
        use super::*;

        #[test]
        fn enter_search_activates_search_mode() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);

            reduce_json(&mut state, &Action::JsonEnterSearch, Instant::now());

            assert!(state.json_detail.search().is_active());
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Searching);
        }

        #[test]
        fn exit_search_deactivates_search_mode() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterSearch, Instant::now());

            reduce_json(&mut state, &Action::JsonExitSearch, Instant::now());

            assert!(!state.json_detail.search().is_active());
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Viewing);
        }

        #[test]
        fn submit_deactivates_and_preserves_matches() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterSearch, Instant::now());

            for ch in "theme".chars() {
                reduce_json(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.json_detail.search().matches().len();
            assert!(match_count > 0, "should find at least one match");

            reduce_json(&mut state, &Action::JsonSearchSubmit, Instant::now());

            assert!(!state.json_detail.search().is_active());
            let expected_cursor = state.json_detail.search().matches()[0];
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Viewing);
            assert_eq!(state.json_detail.editor().cursor(), expected_cursor);
            assert_eq!(
                state.json_detail.editor().cursor_to_position(),
                cursor_position(state.json_detail.editor().content(), expected_cursor)
            );
        }

        #[test]
        fn text_input_updates_search_matches_case_insensitively() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterSearch, Instant::now());

            assert!(state.json_detail.search().matches().is_empty());

            for ch in "THEME".chars() {
                reduce_json(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }

            assert!(
                !state.json_detail.search().matches().is_empty(),
                "should find matches for 'THEME'"
            );
        }

        #[test]
        fn kill_then_yank_restores_search_query_and_matches() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterSearch, Instant::now());

            for ch in "theme".chars() {
                reduce_json(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            reduce_json(
                &mut state,
                &Action::TextKill {
                    target: InputTarget::JsonSearch,
                    direction: TextKillDirection::ToLineStart,
                },
                Instant::now(),
            );
            reduce_json(
                &mut state,
                &Action::TextYank {
                    target: InputTarget::JsonSearch,
                },
                Instant::now(),
            );

            assert_eq!(state.json_detail.search().input().content(), "theme");
            assert!(!state.json_detail.search().matches().is_empty());
        }

        #[test]
        fn next_cycles_through_matches_and_moves_cursor() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterSearch, Instant::now());

            for ch in "t".chars() {
                reduce_json(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.json_detail.search().matches().len();
            assert!(
                match_count > 1,
                "test precondition: need 2+ matches for cycling test, got {match_count}"
            );
            assert_eq!(state.json_detail.search().current_match(), 0);

            reduce_json(&mut state, &Action::JsonSearchNext, Instant::now());

            assert_eq!(state.json_detail.search().current_match(), 1);
            let expected_cursor = state.json_detail.search().matches()[1];
            assert_eq!(state.json_detail.editor().cursor(), expected_cursor);
            assert_eq!(
                state.json_detail.editor().cursor_to_position(),
                cursor_position(state.json_detail.editor().content(), expected_cursor)
            );
        }

        #[test]
        fn prev_wraps_to_last_match_and_moves_cursor() {
            let mut state = state_with_json_cell();
            open_detail(&mut state);
            reduce_json(&mut state, &Action::JsonEnterSearch, Instant::now());

            for ch in "t".chars() {
                reduce_json(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.json_detail.search().matches().len();
            assert!(
                match_count > 1,
                "test precondition: need 2+ matches for wrap test, got {match_count}"
            );
            reduce_json(&mut state, &Action::JsonSearchPrev, Instant::now());

            assert_eq!(state.json_detail.search().current_match(), match_count - 1);
            let expected_cursor = state.json_detail.search().matches()[match_count - 1];
            assert_eq!(state.json_detail.editor().cursor(), expected_cursor);
            assert_eq!(
                state.json_detail.editor().cursor_to_position(),
                cursor_position(state.json_detail.editor().content(), expected_cursor)
            );
        }
    }

    mod reducer_chain {
        use super::*;
        use crate::model::shared::confirm_dialog::ConfirmIntent;
        use crate::update::reducer::reduce as reduce_app;

        #[test]
        fn json_detail_actions_flow_through_top_reducer() {
            let mut state = state_with_json_cell();
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            assert_eq!(state.input_mode(), InputMode::JsonDetail);

            reduce_app(&mut state, Action::JsonEnterSearch, now, &services);
            reduce_app(
                &mut state,
                Action::TextInput {
                    target: InputTarget::JsonSearch,
                    ch: 't',
                },
                now,
                &services,
            );
            assert!(!state.json_detail.search().matches().is_empty());

            reduce_app(&mut state, Action::JsonSearchNext, now, &services);
            reduce_app(&mut state, Action::JsonSearchSubmit, now, &services);
            assert!(!state.json_detail.search().is_active());
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Viewing);

            let effects = reduce_app(&mut state, Action::JsonYankAll, now, &services);
            assert!(matches!(
                effects.first(),
                Some(Effect::CopyToClipboard { content, .. }) if content.contains("theme")
            ));

            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            assert_eq!(state.input_mode(), InputMode::JsonEdit);
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Editing);

            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            assert_eq!(state.input_mode(), InputMode::JsonDetail);

            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.json_detail.is_active());
        }

        #[test]
        fn json_edit_input_actions_flow_through_top_reducer() {
            let mut state = state_with_json_cell();
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            reduce_app(
                &mut state,
                Action::TextInput {
                    target: InputTarget::JsonEdit,
                    ch: ' ',
                },
                now,
                &services,
            );
            reduce_app(
                &mut state,
                Action::TextBackspace {
                    target: InputTarget::JsonEdit,
                },
                now,
                &services,
            );
            reduce_app(
                &mut state,
                Action::TextDelete {
                    target: InputTarget::JsonEdit,
                },
                now,
                &services,
            );
            reduce_app(&mut state, Action::Paste(" ".to_string()), now, &services);

            assert_eq!(state.input_mode(), InputMode::JsonEdit);
            assert_eq!(state.json_detail.mode(), JsonDetailMode::Editing);
        }

        #[test]
        fn json_edit_close_can_continue_to_write_preview() {
            let mut state = state_with_json_cell();
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            state
                .json_detail
                .editor_mut()
                .set_content(r#"{"theme":"light","count":5}"#.to_string());
            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);
            assert!(effects.is_empty());

            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.result_interaction.pending_write_preview().is_some());
            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(ConfirmIntent::ExecuteWrite { blocked: false, .. })
            ));
        }

        #[test]
        fn mysql_json_edit_close_can_continue_to_write_preview() {
            let mut state = state_with_mysql_json_value(r#"{"theme":"dark"}"#);
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            state
                .json_detail
                .editor_mut()
                .set_content(r#"{"theme":"light"}"#.to_string());
            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);
            assert!(effects.is_empty());
            let preview = state
                .result_interaction
                .pending_write_preview()
                .expect("write preview");

            assert_eq!(
                preview.sql,
                "UPDATE `public`.`users`\nSET `settings` = '{\"theme\":\"light\"}'\nWHERE `id` = '1';"
            );
            assert!(preview.diff[0].json_diff.is_some());
        }

        #[test]
        fn mysql_json_edit_uses_explicit_hidden_row_identity() {
            let mut state = state_with_hidden_primary_key_json_value();
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("mysql-test"),
                "mysql",
                DatabaseType::MySQL,
                "mysql://localhost/test",
            );
            let mut detail = state.session.table_detail().cloned().expect("table detail");
            detail.columns[1].data_type = "json".to_string();
            state.session.set_table_detail_raw(Some(detail));
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            state
                .json_detail
                .editor_mut()
                .set_content(r#"{"theme":"light"}"#.to_string());
            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);
            assert!(effects.is_empty());
            let preview = state
                .result_interaction
                .pending_write_preview()
                .expect("write preview");

            assert!(preview.sql.contains("WHERE `id` = '1'"));
            assert!(preview.sql.contains("SET `settings` ="));
            assert!(!preview.sql.contains("SET `id` ="));
            assert_eq!(preview.diff[0].column, "settings");
            assert!(preview.diff[0].json_diff.is_some());
        }

        #[test]
        fn invalid_mysql_json_is_rejected_before_write_preview() {
            let mut state = state_with_mysql_json_value(r#"{"theme":"dark"}"#);
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            state
                .json_detail
                .editor_mut()
                .set_content("{invalid}".to_string());
            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);

            assert!(effects.is_empty());
            assert!(
                state
                    .messages
                    .last_error()
                    .is_some_and(|error| error.starts_with("Invalid JSON:"))
            );
        }

        #[test]
        fn empty_mysql_json_editor_is_rejected_before_write_preview() {
            let mut state = state_with_mysql_json_value(r#"{"theme":"dark"}"#);
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            state.json_detail.editor_mut().set_content(String::new());
            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);

            assert!(effects.is_empty());
            assert!(
                state
                    .messages
                    .last_error()
                    .is_some_and(|error| error.starts_with("Invalid JSON:"))
            );
        }

        #[test]
        fn semantically_unchanged_mysql_json_is_not_written() {
            let mut state = state_with_mysql_json_value(r#"{"a":1,"b":2}"#);
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            state
                .json_detail
                .editor_mut()
                .set_content("{ \"b\": 2, \"a\": 1 }".to_string());
            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);

            assert!(effects.is_empty());
            assert_eq!(
                state.messages.last_error(),
                Some("No semantic changes to write")
            );
        }

        #[test]
        fn mysql_json_null_and_string_null_use_distinct_literals() {
            let mut state = state_with_mysql_json_value("null");
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonEnterEdit, now, &services);
            state
                .json_detail
                .editor_mut()
                .set_content(r#""null""#.to_string());
            reduce_app(&mut state, Action::JsonExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);
            assert!(effects.is_empty());
            let preview = state
                .result_interaction
                .pending_write_preview()
                .expect("write preview");

            assert_eq!(
                preview.sql,
                "UPDATE `public`.`users`\nSET `settings` = '\"null\"'\nWHERE `id` = '1';"
            );
        }
    }
}
