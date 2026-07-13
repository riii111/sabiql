use crate::cmd::effect::Effect;
#[cfg(test)]
use crate::domain::ColumnAttributes;
use crate::domain::QuerySource;
use crate::model::app_state::AppState;
use crate::model::browse::jsonb_detail::JsonbDetailState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::shared::text_input::TextInputLike;
use crate::model::shared::ui_state::DEFAULT_JSONB_DETAIL_EDITOR_VISIBLE_ROWS;
use crate::policy::preview_cell_text::{preview_cell_text_diff_handling, uses_jsonb_detail_modal};
use crate::ports::outbound::ClipboardError;
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::{
    EditGuardrailError, editable_preview_base, ensure_column_writable, find_text_matches,
};
use std::time::Instant;

pub fn reduce_jsonb(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::JsonbDetail) => {
            let result = match state.query.visible_result() {
                Some(r) if r.source == QuerySource::Preview && !r.is_error() => r,
                _ => return DispatchResult::handled(),
            };

            let table_detail = match state.session.table_detail() {
                Some(td)
                    if td.schema == state.query.pagination.schema()
                        && td.name == state.query.pagination.table() =>
                {
                    td
                }
                _ => return DispatchResult::handled(),
            };

            let Some(row_idx) = state.result_interaction.selection().row() else {
                return DispatchResult::handled();
            };
            let Some(col_idx) = state.result_interaction.selection().cell() else {
                return DispatchResult::handled();
            };

            let database_type = state.session.active_database_type_or_default();
            let Some(column) = table_detail.columns.get(col_idx) else {
                return DispatchResult::handled();
            };
            let handling =
                preview_cell_text_diff_handling(database_type, column.data_type.as_str());
            if !uses_jsonb_detail_modal(handling) {
                return DispatchResult::handled();
            }

            let cell_value = match result.display_value_at(row_idx, col_idx) {
                Some(value) if !value.is_empty() => value,
                _ => return DispatchResult::handled(),
            };

            let pretty_original = match serde_json::from_str::<serde_json::Value>(&cell_value) {
                Ok(value) => {
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| cell_value.clone())
                }
                Err(err) => {
                    state
                        .messages
                        .set_error_at(format!("Invalid JSON: {err}"), now);
                    return DispatchResult::handled();
                }
            };

            state.jsonb_detail = JsonbDetailState::open_pretty(
                row_idx,
                col_idx,
                column.name.clone(),
                cell_value,
                pretty_original,
            );
            state.modal.push_mode(InputMode::JsonbDetail);
            DispatchResult::handled()
        }

        Action::CloseModal(ModalKind::JsonbDetail) => {
            apply_pending_edit_as_draft(state);
            state.jsonb_detail.close();
            state.modal.pop_mode();
            DispatchResult::handled()
        }

        Action::JsonbYankAll => {
            let json = state.jsonb_detail.current_json_for_yank();
            DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                content: json,
                on_success: Some(Box::new(Action::JsonbYankSuccess)),
                on_failure: Some(Box::new(Action::CopyFailed(ClipboardError::Unavailable(
                    "Clipboard unavailable".into(),
                )))),
            }])
        }

        Action::JsonbYankSuccess => {
            state.flash_timers.set(FlashId::JsonbDetail, now);
            DispatchResult::handled()
        }

        Action::JsonbEnterEdit => {
            if state.session.is_read_only() {
                state
                    .messages
                    .set_error_at("Read-only mode: editing is disabled".to_string(), now);
                return DispatchResult::handled();
            }
            if let Err(reason) = ensure_jsonb_column_writable(state) {
                state.messages.set_error_at(reason.to_string(), now);
                return DispatchResult::handled();
            }
            state.jsonb_detail.enter_edit();
            state.modal.replace_mode(InputMode::JsonbEdit);
            DispatchResult::handled()
        }

        Action::JsonbAppendInsert => {
            if state.session.is_read_only() {
                state
                    .messages
                    .set_error_at("Read-only mode: editing is disabled".to_string(), now);
                return DispatchResult::handled();
            }
            if let Err(reason) = ensure_jsonb_column_writable(state) {
                state.messages.set_error_at(reason.to_string(), now);
                return DispatchResult::handled();
            }
            state
                .jsonb_detail
                .editor_mut()
                .move_cursor(CursorMove::LineEnd);
            update_editor_scroll(state);
            state.jsonb_detail.enter_edit();
            state.modal.replace_mode(InputMode::JsonbEdit);
            DispatchResult::handled()
        }

        Action::JsonbExitEdit => {
            state.jsonb_detail.exit_edit();
            state.modal.replace_mode(InputMode::JsonbDetail);
            DispatchResult::handled()
        }

        Action::TextInput {
            target: InputTarget::JsonbEdit,
            ch,
        } => {
            if *ch == '\n' {
                state.jsonb_detail.editor_mut().insert_newline();
            } else if *ch == '\t' {
                state.jsonb_detail.editor_mut().insert_tab();
            } else {
                state.jsonb_detail.editor_mut().insert_char(*ch);
            }
            update_editor_scroll(state);
            state.jsonb_detail.validate_editor_content();
            DispatchResult::handled()
        }

        Action::TextBackspace {
            target: InputTarget::JsonbEdit,
        } => {
            state.jsonb_detail.editor_mut().backspace();
            update_editor_scroll(state);
            state.jsonb_detail.validate_editor_content();
            DispatchResult::handled()
        }

        Action::TextDelete {
            target: InputTarget::JsonbEdit,
        } => {
            state.jsonb_detail.editor_mut().delete();
            update_editor_scroll(state);
            state.jsonb_detail.validate_editor_content();
            DispatchResult::handled()
        }

        Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction,
        } => {
            match direction {
                CursorMove::ViewportTop
                | CursorMove::ViewportMiddle
                | CursorMove::ViewportBottom => {
                    let visible_rows = effective_visible_rows(state);
                    state
                        .jsonb_detail
                        .editor_mut()
                        .move_cursor_to_viewport_position(*direction, visible_rows);
                }
                _ => state.jsonb_detail.editor_mut().move_cursor(*direction),
            }
            update_editor_scroll(state);
            state.ui.set_key_sequence(KeySequenceState::Idle);
            DispatchResult::handled()
        }

        Action::Paste(text) if state.input_mode() == InputMode::JsonbEdit => {
            state.jsonb_detail.editor_mut().insert_str(text);
            update_editor_scroll(state);
            state.jsonb_detail.validate_editor_content();
            DispatchResult::handled()
        }

        Action::JsonbEnterSearch => {
            state.jsonb_detail.enter_search();
            DispatchResult::handled()
        }

        Action::JsonbExitSearch => {
            state.jsonb_detail.exit_search();
            DispatchResult::handled()
        }

        Action::JsonbSearchSubmit => {
            state.jsonb_detail.exit_search();
            jump_to_current_match(state);
            DispatchResult::handled()
        }

        Action::JsonbSearchNext => {
            state.jsonb_detail.search_mut().advance_to_next_match();
            jump_to_current_match(state);
            DispatchResult::handled()
        }

        Action::JsonbSearchPrev => {
            state.jsonb_detail.search_mut().advance_to_prev_match();
            jump_to_current_match(state);
            DispatchResult::handled()
        }

        Action::TextInput {
            target: InputTarget::JsonbSearch,
            ch,
        } => {
            state.jsonb_detail.search_mut().input_mut().insert_char(*ch);
            update_search_matches(state);
            DispatchResult::handled()
        }

        Action::TextBackspace {
            target: InputTarget::JsonbSearch,
        } => {
            state.jsonb_detail.search_mut().input_mut().backspace();
            update_search_matches(state);
            DispatchResult::handled()
        }

        Action::TextDelete {
            target: InputTarget::JsonbSearch,
        } => {
            state.jsonb_detail.search_mut().input_mut().delete();
            update_search_matches(state);
            DispatchResult::handled()
        }

        Action::Paste(text)
            if state.input_mode() == InputMode::JsonbDetail
                && state.jsonb_detail.search().is_active() =>
        {
            let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            state
                .jsonb_detail
                .search_mut()
                .input_mut()
                .insert_str(&clean);
            update_search_matches(state);
            DispatchResult::handled()
        }

        Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction,
        } => {
            state
                .jsonb_detail
                .search_mut()
                .input_mut()
                .move_cursor(*direction);
            DispatchResult::handled()
        }

        _ => DispatchResult::pass(),
    }
}

fn ensure_jsonb_column_writable(state: &AppState) -> Result<(), EditGuardrailError> {
    let (_, identity) = editable_preview_base(state)?;
    ensure_column_writable(state, state.jsonb_detail.column_name(), &identity)
}

fn update_search_matches(state: &mut AppState) {
    let query = state.jsonb_detail.search().input().content().to_string();
    let matches = find_text_matches(state.jsonb_detail.editor().content(), &query);
    state.jsonb_detail.search_mut().set_matches(matches);
}

fn jump_to_current_match(state: &mut AppState) {
    let search = state.jsonb_detail.search();
    if let Some(&match_pos) = search.matches().get(search.current_match()) {
        state.jsonb_detail.editor_mut().set_cursor(match_pos);
        update_editor_scroll(state);
    }
}

fn update_editor_scroll(state: &mut AppState) {
    let visible_rows = effective_visible_rows(state);
    state.jsonb_detail.editor_mut().update_scroll(visible_rows);
}

fn effective_visible_rows(state: &AppState) -> usize {
    match state.jsonb_detail_editor_visible_rows() {
        0 => DEFAULT_JSONB_DETAIL_EDITOR_VISIBLE_ROWS,
        rows => rows,
    }
}

fn apply_pending_edit_as_draft(state: &mut AppState) {
    if !state.jsonb_detail.has_pending_changes() {
        return;
    }

    let content = state.jsonb_detail.editor().content().to_string();

    if let Ok(compact) = serde_json::from_str::<serde_json::Value>(&content) {
        let compact_str = serde_json::to_string(&compact).unwrap_or_else(|_| content.clone());
        let row = state.jsonb_detail.row();
        let col = state.jsonb_detail.col();
        let original_cell = state
            .query
            .visible_result()
            .and_then(|result| result.display_value_at(row, col))
            .unwrap_or_default();
        state
            .result_interaction
            .begin_cell_edit(row, col, original_cell);
        state.result_interaction.clear_write_preview();
        state
            .result_interaction
            .replace_cell_edit_draft(compact_str);
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support;

    use super::*;
    pub use crate::domain::Column;
    use crate::domain::{QueryResult, QuerySource, Table};
    use crate::services::AppServices;
    use std::sync::Arc;

    fn jsonb_table() -> Table {
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

    fn state_with_jsonb_cell() -> AppState {
        state_with_jsonb_value(r#"{"theme":"dark","count":5}"#)
    }

    fn state_with_jsonb_value(cell_value: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
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
        state.session.set_table_detail_raw(Some(jsonb_table()));
        state.result_interaction.activate_cell(0, 1);
        state
    }

    fn open_detail(state: &mut AppState) {
        reduce_jsonb(
            state,
            &Action::OpenModal(ModalKind::JsonbDetail),
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
        use crate::domain::DatabaseType;
        use crate::domain::connection::ConnectionId;

        #[test]
        fn opens_on_valid_jsonb_cell() {
            let mut state = state_with_jsonb_cell();

            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert!(state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
        }

        #[test]
        fn blocked_on_non_jsonb_column() {
            let mut state = state_with_jsonb_cell();
            state.result_interaction.move_cell(0);

            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert!(!state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn blocked_on_sqlite_jsonb_column() {
            let mut state = state_with_jsonb_cell();
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("sqlite-test"),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite:///tmp/app.db",
            );

            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert!(!state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn blocked_on_null_cell() {
            let mut state = state_with_jsonb_cell();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    vec!["id".to_string(), "settings".to_string()],
                    vec![vec!["1".to_string(), String::new()]],
                    1,
                    QuerySource::Preview,
                )));

            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert!(!state.jsonb_detail.is_active());
        }

        #[test]
        fn blocked_on_adhoc_result() {
            let mut state = state_with_jsonb_cell();
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    vec!["id".to_string(), "settings".to_string()],
                    vec![vec!["1".to_string(), r#"{"theme":"dark"}"#.to_string()]],
                    1,
                    QuerySource::Adhoc,
                )));

            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert!(!state.jsonb_detail.is_active());
        }

        #[test]
        fn blocked_without_table_detail() {
            let mut state = state_with_jsonb_cell();
            state.session.set_table_detail_raw(None);

            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert!(!state.jsonb_detail.is_active());
        }
    }

    mod navigation {
        use super::*;

        #[test]
        fn close_clears_state() {
            let mut state = state_with_jsonb_cell();
            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );
            assert!(state.jsonb_detail.is_active());

            reduce_jsonb(
                &mut state,
                &Action::CloseModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert!(!state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }
    }

    mod edit_lifecycle {
        use super::*;
        use crate::model::browse::jsonb_detail::JsonbDetailMode;
        use crate::model::shared::key_sequence::Prefix;
        use crate::update::action::CursorMove;
        use rstest::rstest;

        #[test]
        fn enter_edit_switches_to_jsonb_edit_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);

            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbEdit);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Editing);
        }

        #[test]
        fn enter_edit_preserves_cursor_from_normal_mode() {
            let mut state = state_with_jsonb_value(r#"{"items":["admin","writer"]}"#);
            open_detail(&mut state);
            reduce_jsonb(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );
            reduce_jsonb(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Right,
                },
                Instant::now(),
            );
            let expected = state.jsonb_detail.editor().cursor();

            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.jsonb_detail.editor().cursor(), expected);
        }

        #[test]
        fn append_insert_moves_to_current_line_end_before_editing() {
            let mut state = state_with_jsonb_value(r#"{"items":["admin","writer"]}"#);
            open_detail(&mut state);
            state
                .jsonb_detail
                .editor_mut()
                .set_content_with_cursor("abc\ndef".to_string(), 1);

            reduce_jsonb(&mut state, &Action::JsonbAppendInsert, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbEdit);
            assert_eq!(state.jsonb_detail.editor().cursor_to_position(), (0, 3));
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Editing);
        }

        #[test]
        fn enter_edit_blocks_read_only_column() {
            let mut state = state_with_jsonb_cell();
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
                ..jsonb_table()
            }));
            open_detail(&mut state);

            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Viewing);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Read-only column cannot be edited: settings (generated)")
            );
        }

        #[test]
        fn append_insert_blocks_read_only_column() {
            let mut state = state_with_jsonb_cell();
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
                ..jsonb_table()
            }));
            open_detail(&mut state);

            reduce_jsonb(&mut state, &Action::JsonbAppendInsert, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Viewing);
            assert_eq!(
                state.messages.last_error.as_deref(),
                Some("Read-only column cannot be edited: settings (generated)")
            );
        }

        #[test]
        fn movement_updates_scroll_with_current_editor_viewport_height() {
            let mut state = state_with_jsonb_value(r#"{"items":["admin","writer","reader"]}"#);
            state.ui.set_jsonb_detail_editor_visible_rows(2);
            open_detail(&mut state);

            reduce_jsonb(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );
            reduce_jsonb(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                },
                Instant::now(),
            );

            assert_eq!(state.jsonb_detail.editor().scroll_row(), 1);
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
                state_with_jsonb_value("{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3,\n  \"d\": 4\n}");
            state.ui.set_jsonb_detail_editor_visible_rows(3);
            open_detail(&mut state);
            state.modal.replace_mode(InputMode::JsonbEdit);
            state.jsonb_detail.set_mode(JsonbDetailMode::Editing);
            state
                .jsonb_detail
                .editor_mut()
                .set_content_with_cursor("line1\nline2\nline3\nline4".to_string(), 0);

            reduce_jsonb(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: movement,
                },
                Instant::now(),
            );

            assert_eq!(
                state.jsonb_detail.editor().cursor_to_position().0,
                expected_row
            );
        }

        #[test]
        fn cursor_movement_clears_pending_key_sequence() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            state.modal.replace_mode(InputMode::JsonbEdit);
            state.jsonb_detail.set_mode(JsonbDetailMode::Editing);
            state
                .ui
                .set_key_sequence(KeySequenceState::WaitingSecondKey(Prefix::G));

            reduce_jsonb(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Right,
                },
                Instant::now(),
            );

            assert_eq!(state.ui.key_sequence().pending_prefix(), None);
        }

        #[test]
        fn enter_edit_blocked_in_read_only_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            state.session.enable_read_only();

            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn append_insert_blocked_in_read_only_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            state.session.enable_read_only();
            let cursor_before = state.jsonb_detail.editor().cursor();

            reduce_jsonb(&mut state, &Action::JsonbAppendInsert, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
            assert_eq!(state.jsonb_detail.editor().cursor(), cursor_before);
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn exit_edit_returns_to_viewing_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());

            reduce_jsonb(&mut state, &Action::JsonbExitEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Viewing);
            assert!(state.jsonb_detail.is_active());
        }

        #[test]
        fn reenter_edit_with_pending_changes_preserves_existing_cursor() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());
            state
                .jsonb_detail
                .editor_mut()
                .set_content_with_cursor(r#"{"theme":"light","count":5}"#.to_string(), 7);
            reduce_jsonb(&mut state, &Action::JsonbExitEdit, Instant::now());

            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.jsonb_detail.editor().cursor(), 7);
        }

        #[test]
        fn close_after_edit_with_valid_changes_stores_draft() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());
            state
                .jsonb_detail
                .editor_mut()
                .set_content(r#"{"theme":"light","count":5}"#.to_string());
            reduce_jsonb(&mut state, &Action::JsonbExitEdit, Instant::now());

            reduce_jsonb(
                &mut state,
                &Action::CloseModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.jsonb_detail.is_active());
            assert!(state.result_interaction.cell_edit().has_pending_draft());
        }

        #[test]
        fn close_after_edit_without_changes_no_draft() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterEdit, Instant::now());
            reduce_jsonb(&mut state, &Action::JsonbExitEdit, Instant::now());

            reduce_jsonb(
                &mut state,
                &Action::CloseModal(ModalKind::JsonbDetail),
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
            let mut state = state_with_jsonb_cell();
            reduce_jsonb(
                &mut state,
                &Action::OpenModal(ModalKind::JsonbDetail),
                Instant::now(),
            );

            let now = Instant::now();
            let effects = reduce_jsonb(&mut state, &Action::JsonbYankAll, now);

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
                        Some(Action::JsonbYankSuccess)
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
            assert!(!state.flash_timers.is_active(FlashId::JsonbDetail, now));
        }

        #[test]
        fn success_sets_flash() {
            let mut state = state_with_jsonb_cell();
            let now = Instant::now();

            reduce_jsonb(&mut state, &Action::JsonbYankSuccess, now);

            assert!(state.flash_timers.is_active(FlashId::JsonbDetail, now));
        }
    }

    mod search {
        use super::*;
        use crate::model::browse::jsonb_detail::JsonbDetailMode;

        #[test]
        fn enter_search_activates_search_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);

            reduce_jsonb(&mut state, &Action::JsonbEnterSearch, Instant::now());

            assert!(state.jsonb_detail.search().is_active());
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Searching);
        }

        #[test]
        fn exit_search_deactivates_search_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterSearch, Instant::now());

            reduce_jsonb(&mut state, &Action::JsonbExitSearch, Instant::now());

            assert!(!state.jsonb_detail.search().is_active());
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Viewing);
        }

        #[test]
        fn submit_deactivates_and_preserves_matches() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterSearch, Instant::now());

            for ch in "theme".chars() {
                reduce_jsonb(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.jsonb_detail.search().matches().len();
            assert!(match_count > 0, "should find at least one match");

            reduce_jsonb(&mut state, &Action::JsonbSearchSubmit, Instant::now());

            assert!(!state.jsonb_detail.search().is_active());
            let expected_cursor = state.jsonb_detail.search().matches()[0];
            assert_eq!(state.jsonb_detail.editor().cursor(), expected_cursor);
            assert_eq!(
                state.jsonb_detail.editor().cursor_to_position(),
                cursor_position(state.jsonb_detail.editor().content(), expected_cursor)
            );
        }

        #[test]
        fn text_input_updates_search_matches_case_insensitively() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterSearch, Instant::now());

            assert!(state.jsonb_detail.search().matches().is_empty());

            for ch in "THEME".chars() {
                reduce_jsonb(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }

            assert!(
                !state.jsonb_detail.search().matches().is_empty(),
                "should find matches for 'THEME'"
            );
        }

        #[test]
        fn next_cycles_through_matches_and_moves_cursor() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterSearch, Instant::now());

            for ch in "t".chars() {
                reduce_jsonb(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.jsonb_detail.search().matches().len();
            assert!(
                match_count > 1,
                "test precondition: need 2+ matches for cycling test, got {match_count}"
            );
            assert_eq!(state.jsonb_detail.search().current_match(), 0);

            reduce_jsonb(&mut state, &Action::JsonbSearchNext, Instant::now());

            assert_eq!(state.jsonb_detail.search().current_match(), 1);
            let expected_cursor = state.jsonb_detail.search().matches()[1];
            assert_eq!(state.jsonb_detail.editor().cursor(), expected_cursor);
            assert_eq!(
                state.jsonb_detail.editor().cursor_to_position(),
                cursor_position(state.jsonb_detail.editor().content(), expected_cursor)
            );
        }

        #[test]
        fn prev_wraps_to_last_match_and_moves_cursor() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce_jsonb(&mut state, &Action::JsonbEnterSearch, Instant::now());

            for ch in "t".chars() {
                reduce_jsonb(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.jsonb_detail.search().matches().len();
            assert!(
                match_count > 1,
                "test precondition: need 2+ matches for wrap test, got {match_count}"
            );
            reduce_jsonb(&mut state, &Action::JsonbSearchPrev, Instant::now());

            assert_eq!(state.jsonb_detail.search().current_match(), match_count - 1);
            let expected_cursor = state.jsonb_detail.search().matches()[match_count - 1];
            assert_eq!(state.jsonb_detail.editor().cursor(), expected_cursor);
            assert_eq!(
                state.jsonb_detail.editor().cursor_to_position(),
                cursor_position(state.jsonb_detail.editor().content(), expected_cursor)
            );
        }
    }

    mod reducer_chain {
        use super::*;
        use crate::model::browse::jsonb_detail::JsonbDetailMode;
        use crate::model::shared::confirm_dialog::ConfirmIntent;
        use crate::update::reducer::reduce as reduce_app;

        #[test]
        fn jsonb_detail_actions_flow_through_top_reducer() {
            let mut state = state_with_jsonb_cell();
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonbDetail),
                now,
                &services,
            );
            assert_eq!(state.input_mode(), InputMode::JsonbDetail);

            reduce_app(&mut state, Action::JsonbEnterSearch, now, &services);
            reduce_app(
                &mut state,
                Action::TextInput {
                    target: InputTarget::JsonbSearch,
                    ch: 't',
                },
                now,
                &services,
            );
            assert!(!state.jsonb_detail.search().matches().is_empty());

            reduce_app(&mut state, Action::JsonbSearchNext, now, &services);
            reduce_app(&mut state, Action::JsonbSearchSubmit, now, &services);
            assert!(!state.jsonb_detail.search().is_active());

            let effects = reduce_app(&mut state, Action::JsonbYankAll, now, &services);
            assert!(matches!(
                effects.first(),
                Some(Effect::CopyToClipboard { content, .. }) if content.contains("theme")
            ));

            reduce_app(&mut state, Action::JsonbEnterEdit, now, &services);
            assert_eq!(state.input_mode(), InputMode::JsonbEdit);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Editing);

            reduce_app(&mut state, Action::JsonbExitEdit, now, &services);
            assert_eq!(state.input_mode(), InputMode::JsonbDetail);

            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonbDetail),
                now,
                &services,
            );
            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.jsonb_detail.is_active());
        }

        #[test]
        fn jsonb_edit_input_actions_flow_through_top_reducer() {
            let mut state = state_with_jsonb_cell();
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonbDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonbEnterEdit, now, &services);
            reduce_app(
                &mut state,
                Action::TextInput {
                    target: InputTarget::JsonbEdit,
                    ch: ' ',
                },
                now,
                &services,
            );
            reduce_app(
                &mut state,
                Action::TextBackspace {
                    target: InputTarget::JsonbEdit,
                },
                now,
                &services,
            );
            reduce_app(
                &mut state,
                Action::TextDelete {
                    target: InputTarget::JsonbEdit,
                },
                now,
                &services,
            );
            reduce_app(&mut state, Action::Paste(" ".to_string()), now, &services);

            assert_eq!(state.input_mode(), InputMode::JsonbEdit);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Editing);
        }

        #[test]
        fn jsonb_edit_close_can_continue_to_write_preview() {
            let mut state = state_with_jsonb_cell();
            let services = AppServices::stub();
            let now = Instant::now();

            reduce_app(
                &mut state,
                Action::OpenModal(ModalKind::JsonbDetail),
                now,
                &services,
            );
            reduce_app(&mut state, Action::JsonbEnterEdit, now, &services);
            state
                .jsonb_detail
                .editor_mut()
                .set_content(r#"{"theme":"light","count":5}"#.to_string());
            reduce_app(&mut state, Action::JsonbExitEdit, now, &services);
            reduce_app(
                &mut state,
                Action::CloseModal(ModalKind::JsonbDetail),
                now,
                &services,
            );

            let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);
            let preview = match effects.first() {
                Some(Effect::DispatchActions(actions)) => match actions.first() {
                    Some(Action::OpenWritePreviewConfirm(preview)) => preview.clone(),
                    other => panic!("expected OpenWritePreviewConfirm, got {other:?}"),
                },
                other => panic!("expected DispatchActions, got {other:?}"),
            };

            reduce_app(
                &mut state,
                Action::OpenWritePreviewConfirm(preview),
                now,
                &services,
            );

            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.result_interaction.pending_write_preview().is_some());
            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(ConfirmIntent::ExecuteWrite { blocked: false, .. })
            ));
        }
    }
}
