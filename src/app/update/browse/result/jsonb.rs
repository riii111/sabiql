use std::time::Instant;
use unicode_casefold::UnicodeCaseFold;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::text_input::TextInputLike;
use crate::app::model::shared::ui_state::DEFAULT_JSONB_DETAIL_EDITOR_VISIBLE_ROWS;
use crate::app::update::action::{Action, InputTarget};
use crate::domain::QuerySource;

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenJsonbDetail => {
            let result = match state.query.visible_result() {
                Some(r) if r.source == QuerySource::Preview && !r.is_error() => r,
                _ => return Some(vec![]),
            };

            if state.query.is_history_mode() {
                return Some(vec![]);
            }

            let table_detail = match state.session.table_detail() {
                Some(td)
                    if td.schema == state.query.pagination.schema
                        && td.name == state.query.pagination.table =>
                {
                    td
                }
                _ => return Some(vec![]),
            };

            let Some(row_idx) = state.result_interaction.selection().row() else {
                return Some(vec![]);
            };
            let Some(col_idx) = state.result_interaction.selection().cell() else {
                return Some(vec![]);
            };

            let column = match table_detail.columns.get(col_idx) {
                Some(c) if c.data_type == "jsonb" => c,
                _ => return Some(vec![]),
            };

            let cell_value = match result.rows.get(row_idx).and_then(|r| r.get(col_idx)) {
                Some(v) if !v.is_empty() => v,
                _ => return Some(vec![]),
            };

            let pretty_original = match serde_json::from_str::<serde_json::Value>(cell_value) {
                Ok(value) => {
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| cell_value.clone())
                }
                Err(err) => {
                    state
                        .messages
                        .set_error_at(format!("Invalid JSON: {err}"), now);
                    return Some(vec![]);
                }
            };

            state.jsonb_detail = JsonbDetailState::open_pretty(
                row_idx,
                col_idx,
                column.name.clone(),
                cell_value.clone(),
                pretty_original,
            );
            state.modal.push_mode(InputMode::JsonbDetail);
            Some(vec![])
        }

        Action::CloseJsonbDetail => {
            apply_pending_edit_as_draft(state);
            state.jsonb_detail.close();
            state.modal.pop_mode();
            Some(vec![])
        }

        Action::JsonbYankAll => {
            let json = state.jsonb_detail.current_json_for_yank();
            state.flash_timers.set(
                crate::app::model::shared::flash_timer::FlashId::JsonbDetail,
                now,
            );
            Some(vec![Effect::CopyToClipboard {
                content: json,
                on_success: Some(Action::CellCopied),
                on_failure: Some(Action::CopyFailed(crate::app::ports::ClipboardError {
                    message: "Clipboard unavailable".into(),
                })),
            }])
        }

        Action::JsonbEnterEdit => {
            if state.session.read_only {
                state
                    .messages
                    .set_error_at("Read-only mode: editing is disabled".to_string(), now);
                return Some(vec![]);
            }
            state.jsonb_detail.enter_edit();
            state.modal.replace_mode(InputMode::JsonbEdit);
            Some(vec![])
        }

        Action::JsonbExitEdit => {
            state.jsonb_detail.exit_edit();
            state.modal.replace_mode(InputMode::JsonbDetail);
            Some(vec![])
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
            validate_editor_inline(state);
            Some(vec![])
        }

        Action::TextBackspace {
            target: InputTarget::JsonbEdit,
        } => {
            state.jsonb_detail.editor_mut().backspace();
            update_editor_scroll(state);
            validate_editor_inline(state);
            Some(vec![])
        }

        Action::TextDelete {
            target: InputTarget::JsonbEdit,
        } => {
            state.jsonb_detail.editor_mut().delete();
            update_editor_scroll(state);
            validate_editor_inline(state);
            Some(vec![])
        }

        Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction,
        } => {
            state.jsonb_detail.editor_mut().move_cursor(*direction);
            update_editor_scroll(state);
            Some(vec![])
        }

        Action::Paste(text) if state.input_mode() == InputMode::JsonbEdit => {
            state.jsonb_detail.editor_mut().insert_str(text);
            update_editor_scroll(state);
            validate_editor_inline(state);
            Some(vec![])
        }

        Action::JsonbEnterSearch => {
            state.jsonb_detail.enter_search();
            Some(vec![])
        }

        Action::JsonbExitSearch => {
            state.jsonb_detail.exit_search();
            Some(vec![])
        }

        Action::JsonbSearchSubmit => {
            state.jsonb_detail.exit_search();
            jump_to_current_match(state);
            Some(vec![])
        }

        Action::JsonbSearchNext => {
            let search = state.jsonb_detail.search();
            if !search.matches.is_empty() {
                let next = (search.current_match + 1) % search.matches.len();
                state.jsonb_detail.search_mut().current_match = next;
                jump_to_current_match(state);
            }
            Some(vec![])
        }

        Action::JsonbSearchPrev => {
            let search = state.jsonb_detail.search();
            if !search.matches.is_empty() {
                let prev = if search.current_match == 0 {
                    search.matches.len() - 1
                } else {
                    search.current_match - 1
                };
                state.jsonb_detail.search_mut().current_match = prev;
                jump_to_current_match(state);
            }
            Some(vec![])
        }

        Action::TextInput {
            target: InputTarget::JsonbSearch,
            ch,
        } => {
            state.jsonb_detail.search_mut().input.insert_char(*ch);
            update_search_matches(state);
            Some(vec![])
        }

        Action::TextBackspace {
            target: InputTarget::JsonbSearch,
        } => {
            state.jsonb_detail.search_mut().input.backspace();
            update_search_matches(state);
            Some(vec![])
        }

        Action::TextDelete {
            target: InputTarget::JsonbSearch,
        } => {
            state.jsonb_detail.search_mut().input.delete();
            update_search_matches(state);
            Some(vec![])
        }

        Action::Paste(text)
            if state.input_mode() == InputMode::JsonbDetail
                && state.jsonb_detail.search().active =>
        {
            let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            state.jsonb_detail.search_mut().input.insert_str(&clean);
            update_search_matches(state);
            Some(vec![])
        }

        Action::TextMoveCursor {
            target: InputTarget::JsonbSearch,
            direction,
        } => {
            state
                .jsonb_detail
                .search_mut()
                .input
                .move_cursor(*direction);
            Some(vec![])
        }

        _ => None,
    }
}

fn update_search_matches(state: &mut AppState) {
    let query = state.jsonb_detail.search().input.content().to_string();
    let matches = find_text_matches(state.jsonb_detail.editor().content(), &query);
    state.jsonb_detail.search_mut().matches = matches;
    state.jsonb_detail.search_mut().current_match = 0;
}

fn jump_to_current_match(state: &mut AppState) {
    let search = state.jsonb_detail.search();
    if let Some(&match_pos) = search.matches.get(search.current_match) {
        state
            .jsonb_detail
            .editor_mut()
            .text_input_mut()
            .set_cursor(match_pos);
        update_editor_scroll(state);
    }
}

fn update_editor_scroll(state: &mut AppState) {
    let visible_rows = match state.jsonb_detail_editor_visible_rows() {
        0 => DEFAULT_JSONB_DETAIL_EDITOR_VISIBLE_ROWS,
        rows => rows,
    };
    state.jsonb_detail.editor_mut().update_scroll(visible_rows);
}

fn find_text_matches(content: &str, query: &str) -> Vec<usize> {
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
            search_from = match_idx + query_folded.len();
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
            .and_then(|r| r.rows.get(row).and_then(|r| r.get(col)).cloned())
            .unwrap_or_default();
        state
            .result_interaction
            .begin_cell_edit(row, col, original_cell);
        state.result_interaction.clear_write_preview();
        state
            .result_interaction
            .cell_edit_input_mut()
            .set_content(compact_str);
    }
}

fn validate_editor_inline(state: &mut AppState) {
    let content = state.jsonb_detail.editor().content().to_string();
    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(_) => state.jsonb_detail.set_validation_error(None),
        Err(e) => state
            .jsonb_detail
            .set_validation_error(Some(format!("Invalid JSON: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::column::Column;
    use crate::domain::{QueryResult, QuerySource, Table};
    use std::sync::Arc;

    fn jsonb_table() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            owner: None,
            columns: vec![
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
                    name: "settings".to_string(),
                    data_type: "jsonb".to_string(),
                    nullable: true,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            indexes: vec![],
            rls: None,
            triggers: vec![],
            row_count_estimate: None,
            comment: None,
        }
    }

    fn state_with_jsonb_cell() -> AppState {
        state_with_jsonb_value(r#"{"theme":"dark","count":5}"#)
    }

    fn state_with_jsonb_value(cell_value: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.query.set_current_result(Arc::new(QueryResult {
            query: String::new(),
            columns: vec!["id".to_string(), "settings".to_string()],
            rows: vec![vec!["1".to_string(), cell_value.to_string()]],
            row_count: 1,
            execution_time_ms: 1,
            executed_at: Instant::now(),
            source: QuerySource::Preview,
            error: None,
            command_tag: None,
        }));
        state.query.pagination.schema = "public".to_string();
        state.query.pagination.table = "users".to_string();
        state.session.set_table_detail_raw(Some(jsonb_table()));
        state.result_interaction.activate_cell(0, 1);
        state
    }

    fn open_detail(state: &mut AppState) {
        reduce(state, &Action::OpenJsonbDetail, Instant::now());
    }

    mod entry_guards {
        use super::*;

        #[test]
        fn opens_on_valid_jsonb_cell() {
            let mut state = state_with_jsonb_cell();

            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            assert!(state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
        }

        #[test]
        fn blocked_on_non_jsonb_column() {
            let mut state = state_with_jsonb_cell();
            state.result_interaction.move_cell(0);

            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            assert!(!state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn blocked_on_null_cell() {
            let mut state = state_with_jsonb_cell();
            state.query.set_current_result(Arc::new(QueryResult {
                query: String::new(),
                columns: vec!["id".to_string(), "settings".to_string()],
                rows: vec![vec!["1".to_string(), String::new()]],
                row_count: 1,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: QuerySource::Preview,
                error: None,
                command_tag: None,
            }));

            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            assert!(!state.jsonb_detail.is_active());
        }

        #[test]
        fn blocked_on_adhoc_result() {
            let mut state = state_with_jsonb_cell();
            state.query.set_current_result(Arc::new(QueryResult {
                query: String::new(),
                columns: vec!["id".to_string(), "settings".to_string()],
                rows: vec![vec!["1".to_string(), r#"{"theme":"dark"}"#.to_string()]],
                row_count: 1,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: QuerySource::Adhoc,
                error: None,
                command_tag: None,
            }));

            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            assert!(!state.jsonb_detail.is_active());
        }

        #[test]
        fn blocked_without_table_detail() {
            let mut state = state_with_jsonb_cell();
            state.session.set_table_detail_raw(None);

            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            assert!(!state.jsonb_detail.is_active());
        }
    }

    mod navigation {
        use super::*;

        #[test]
        fn close_clears_state() {
            let mut state = state_with_jsonb_cell();
            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());
            assert!(state.jsonb_detail.is_active());

            reduce(&mut state, &Action::CloseJsonbDetail, Instant::now());

            assert!(!state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }
    }

    mod edit_lifecycle {
        use super::*;
        use crate::app::model::browse::jsonb_detail::JsonbDetailMode;

        #[test]
        fn enter_edit_switches_to_jsonb_edit_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);

            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbEdit);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Editing);
        }

        #[test]
        fn enter_edit_preserves_cursor_from_normal_mode() {
            let mut state = state_with_jsonb_value(r#"{"items":["admin","writer"]}"#);
            open_detail(&mut state);
            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: crate::app::update::action::CursorMove::Down,
                },
                Instant::now(),
            );
            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: crate::app::update::action::CursorMove::Right,
                },
                Instant::now(),
            );
            let expected = state.jsonb_detail.editor().cursor();

            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.jsonb_detail.editor().cursor(), expected);
        }

        #[test]
        fn movement_updates_scroll_with_current_editor_viewport_height() {
            let mut state = state_with_jsonb_value(r#"{"items":["admin","writer","reader"]}"#);
            state.ui.jsonb_detail_editor_visible_rows = 2;
            open_detail(&mut state);

            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: crate::app::update::action::CursorMove::Down,
                },
                Instant::now(),
            );
            reduce(
                &mut state,
                &Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: crate::app::update::action::CursorMove::Down,
                },
                Instant::now(),
            );

            assert_eq!(state.jsonb_detail.editor().scroll_row(), 1);
        }

        #[test]
        fn enter_edit_blocked_in_read_only_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            state.session.read_only = true;

            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn exit_edit_returns_to_viewing_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());

            reduce(&mut state, &Action::JsonbExitEdit, Instant::now());

            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Viewing);
            assert!(state.jsonb_detail.is_active());
        }

        #[test]
        fn reenter_edit_with_pending_changes_preserves_existing_cursor() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());
            state
                .jsonb_detail
                .editor_mut()
                .set_content_with_cursor(r#"{"theme":"light","count":5}"#.to_string(), 7);
            reduce(&mut state, &Action::JsonbExitEdit, Instant::now());

            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());

            assert_eq!(state.jsonb_detail.editor().cursor(), 7);
        }

        #[test]
        fn close_after_edit_with_valid_changes_stores_draft() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());
            state
                .jsonb_detail
                .editor_mut()
                .set_content(r#"{"theme":"light","count":5}"#.to_string());
            reduce(&mut state, &Action::JsonbExitEdit, Instant::now());

            reduce(&mut state, &Action::CloseJsonbDetail, Instant::now());

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.jsonb_detail.is_active());
            assert!(state.result_interaction.cell_edit().has_pending_draft());
        }

        #[test]
        fn close_after_edit_without_changes_no_draft() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterEdit, Instant::now());
            reduce(&mut state, &Action::JsonbExitEdit, Instant::now());

            reduce(&mut state, &Action::CloseJsonbDetail, Instant::now());

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(!state.result_interaction.cell_edit().has_pending_draft());
        }
    }

    mod yank {
        use super::*;
        use crate::app::cmd::effect::Effect;

        #[test]
        fn copies_all_text_to_clipboard() {
            let mut state = state_with_jsonb_cell();
            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            let effects = reduce(&mut state, &Action::JsonbYankAll, Instant::now());

            let effects = effects.expect("should return effects");
            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content.contains("theme"))
            );
        }
    }

    mod search {
        use super::*;
        use crate::app::model::browse::jsonb_detail::JsonbDetailMode;

        #[test]
        fn enter_search_activates_search_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);

            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            assert!(state.jsonb_detail.search().active);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Searching);
        }

        #[test]
        fn exit_search_deactivates_search_mode() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            reduce(&mut state, &Action::JsonbExitSearch, Instant::now());

            assert!(!state.jsonb_detail.search().active);
            assert_eq!(state.jsonb_detail.mode(), JsonbDetailMode::Viewing);
        }

        #[test]
        fn submit_deactivates_and_preserves_matches() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            for ch in "theme".chars() {
                reduce(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.jsonb_detail.search().matches.len();
            assert!(match_count > 0, "should find at least one match");

            reduce(&mut state, &Action::JsonbSearchSubmit, Instant::now());

            assert!(!state.jsonb_detail.search().active);
            assert_eq!(
                state.jsonb_detail.editor().cursor(),
                state.jsonb_detail.search().matches[0]
            );
        }

        #[test]
        fn text_input_updates_search_matches_case_insensitively() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            assert!(state.jsonb_detail.search().matches.is_empty());

            for ch in "THEME".chars() {
                reduce(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }

            assert!(
                !state.jsonb_detail.search().matches.is_empty(),
                "should find matches for 'THEME'"
            );
        }

        #[test]
        fn next_cycles_through_matches_and_moves_cursor() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            for ch in "t".chars() {
                reduce(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.jsonb_detail.search().matches.len();
            assert!(
                match_count > 1,
                "test precondition: need 2+ matches for cycling test, got {match_count}"
            );
            assert_eq!(state.jsonb_detail.search().current_match, 0);

            reduce(&mut state, &Action::JsonbSearchNext, Instant::now());

            assert_eq!(state.jsonb_detail.search().current_match, 1);
            assert_eq!(
                state.jsonb_detail.editor().cursor(),
                state.jsonb_detail.search().matches[1]
            );
        }

        #[test]
        fn prev_wraps_to_last_match_and_moves_cursor() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            for ch in "t".chars() {
                reduce(
                    &mut state,
                    &Action::TextInput {
                        target: InputTarget::JsonbSearch,
                        ch,
                    },
                    Instant::now(),
                );
            }
            let match_count = state.jsonb_detail.search().matches.len();
            assert!(
                match_count > 1,
                "test precondition: need 2+ matches for wrap test, got {match_count}"
            );
            reduce(&mut state, &Action::JsonbSearchPrev, Instant::now());

            assert_eq!(state.jsonb_detail.search().current_match, match_count - 1);
            assert_eq!(
                state.jsonb_detail.editor().cursor(),
                state.jsonb_detail.search().matches[match_count - 1]
            );
        }
    }

    mod search_helpers {
        use super::{
            casefold_with_char_offsets, find_text_matches, original_char_offset_for_folded_byte,
        };

        #[test]
        fn returns_first_match_offset_per_line_case_insensitively() {
            let matches = find_text_matches(
                "{\n  \"Theme\": \"dark\",\n  \"theme\": \"light\"\n}",
                "theme",
            );

            assert_eq!(matches, vec![5, 24]);
        }

        #[test]
        fn empty_query_returns_no_matches() {
            let matches = find_text_matches("{\n  \"theme\": \"dark\"\n}", "");

            assert!(matches.is_empty());
        }

        #[test]
        fn unicode_casefold_match_maps_back_to_original_char_offset() {
            let matches = find_text_matches("İx", "x");

            assert_eq!(matches, vec![1]);
        }

        #[test]
        fn folded_byte_offset_uses_original_char_positions() {
            let (folded, offset_map) = casefold_with_char_offsets("İx");
            let match_idx = folded.find('x').expect("x should exist after case fold");

            assert_eq!(
                original_char_offset_for_folded_byte(&offset_map, match_idx),
                1
            );
        }

        #[test]
        fn casefold_matches_german_sharp_s() {
            let matches = find_text_matches("Maße", "MASSE");

            assert_eq!(matches, vec![0]);
        }

        #[test]
        fn casefold_matches_greek_final_sigma() {
            let matches = find_text_matches("ὈΔΥΣΣΕΎΣ", "ὀδυσσεύς");

            assert_eq!(matches, vec![0]);
        }

        #[test]
        fn returns_all_matches_within_single_line() {
            let matches = find_text_matches("theme theme", "theme");

            assert_eq!(matches, vec![0, 6]);
        }
    }
}
