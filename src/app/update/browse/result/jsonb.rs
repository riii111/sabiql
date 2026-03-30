use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::policy::json::{parse_json_tree, visible_line_indices};
use crate::app::update::action::{Action, InputTarget};
use crate::domain::QuerySource;

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenJsonbDetail => {
            // Entry guard 1: must be LivePreview
            let result = state.query.current_result().map(AsRef::as_ref);
            let result = match result {
                Some(r) if r.source == QuerySource::Preview && !r.is_error() => r,
                _ => return Some(vec![]),
            };

            if state.query.is_history_mode() {
                return Some(vec![]);
            }

            // Entry guard 2: must have table_detail matching preview target
            let table_detail = match state.session.table_detail() {
                Some(td)
                    if td.schema == state.query.pagination.schema
                        && td.name == state.query.pagination.table =>
                {
                    td
                }
                _ => return Some(vec![]),
            };

            // Entry guard 3: need active cell selection
            let Some(row_idx) = state.result_interaction.selection().row() else {
                return Some(vec![]);
            };
            let Some(col_idx) = state.result_interaction.selection().cell() else {
                return Some(vec![]);
            };

            // Entry guard 4: column must be jsonb
            let column = match table_detail.columns.get(col_idx) {
                Some(c) if c.data_type == "jsonb" => c,
                _ => return Some(vec![]),
            };

            // Entry guard 5: cell value must not be empty (NULL)
            let cell_value = match result.rows.get(row_idx).and_then(|r| r.get(col_idx)) {
                Some(v) if !v.is_empty() => v,
                _ => return Some(vec![]),
            };

            // Parse JSON
            let tree = match parse_json_tree(cell_value) {
                Ok(t) => t,
                Err(msg) => {
                    state.messages.set_error_at(msg, now);
                    return Some(vec![]);
                }
            };

            state.jsonb_detail = JsonbDetailState::open(
                row_idx,
                col_idx,
                column.name.clone(),
                cell_value.clone(),
                tree,
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

        Action::JsonbCursorUp => {
            let vc = state.jsonb_detail.visible_count();
            state.jsonb_detail.cursor_up(vc);
            Some(vec![])
        }

        Action::JsonbCursorDown => {
            let vc = state.jsonb_detail.visible_count();
            state.jsonb_detail.cursor_down(vc);
            Some(vec![])
        }

        Action::JsonbScrollToTop => {
            state.jsonb_detail.cursor_to_top();
            Some(vec![])
        }

        Action::JsonbScrollToEnd => {
            let vc = state.jsonb_detail.visible_count();
            state.jsonb_detail.cursor_to_end(vc);
            Some(vec![])
        }

        Action::JsonbToggleFold => {
            let selected = state.jsonb_detail.selected_line();
            state.jsonb_detail.toggle_fold(selected);
            let vc = state.jsonb_detail.visible_count();
            state.jsonb_detail.clamp_cursor(vc);
            state.jsonb_detail.clamp_scroll(vc);
            if state.jsonb_detail.search().active {
                update_search_matches(state);
            }
            Some(vec![])
        }

        Action::JsonbFoldAll => {
            state.jsonb_detail.fold_all();
            if state.jsonb_detail.search().active {
                update_search_matches(state);
            }
            Some(vec![])
        }

        Action::JsonbUnfoldAll => {
            state.jsonb_detail.unfold_all();
            if state.jsonb_detail.search().active {
                update_search_matches(state);
            }
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

        // ── Edit lifecycle ──────────────────────────────────────────
        Action::JsonbEnterEdit => {
            if state.session.read_only {
                state
                    .messages
                    .set_error_at("Read-only mode: editing is disabled".to_string(), now);
                return Some(vec![]);
            }
            if state.jsonb_detail.has_pending_changes() {
                // Re-enter with existing draft content intact
                state
                    .jsonb_detail
                    .set_mode(crate::app::model::browse::jsonb_detail::JsonbDetailMode::Editing);
            } else {
                let pretty = state.jsonb_detail.pretty_original().to_string();
                let visible = visible_line_indices(state.jsonb_detail.tree());
                let target_line = visible
                    .get(state.jsonb_detail.selected_line())
                    .copied()
                    .unwrap_or(0);
                state.jsonb_detail.enter_edit(pretty, target_line);
            }
            state.modal.replace_mode(InputMode::JsonbEdit);
            Some(vec![])
        }

        Action::JsonbExitEdit => {
            // Rebuild tree from editor content if valid, so viewing/search/yank
            // reflect the draft rather than the original
            let content = state.jsonb_detail.editor().content().to_string();
            if let Ok(tree) = parse_json_tree(&content) {
                state.jsonb_detail.replace_tree(tree);
            }
            state.jsonb_detail.exit_edit();
            state.modal.replace_mode(InputMode::JsonbDetail);
            Some(vec![])
        }

        // ── Text input for JsonbEdit ────────────────────────────────
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
            validate_editor_inline(state);
            Some(vec![])
        }

        Action::TextBackspace {
            target: InputTarget::JsonbEdit,
        } => {
            state.jsonb_detail.editor_mut().backspace();
            validate_editor_inline(state);
            Some(vec![])
        }

        Action::TextDelete {
            target: InputTarget::JsonbEdit,
        } => {
            state.jsonb_detail.editor_mut().delete();
            validate_editor_inline(state);
            Some(vec![])
        }

        Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction,
        } => {
            state.jsonb_detail.editor_mut().move_cursor(*direction);
            Some(vec![])
        }

        Action::Paste(text) if state.input_mode() == InputMode::JsonbEdit => {
            state.jsonb_detail.editor_mut().insert_str(text);
            validate_editor_inline(state);
            Some(vec![])
        }

        // ── Search ──────────────────────────────────────────────────
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
    let indices = state.jsonb_detail.visible_indices();
    let matches =
        crate::app::policy::json::find_matches(state.jsonb_detail.tree(), indices, &query);
    state.jsonb_detail.search_mut().matches = matches;
    state.jsonb_detail.search_mut().current_match = 0;
}

fn jump_to_current_match(state: &mut AppState) {
    let search = state.jsonb_detail.search();
    if let Some(&match_real_idx) = search.matches.get(search.current_match) {
        let indices = state.jsonb_detail.visible_indices();
        if let Ok(visible_pos) = indices.binary_search(&match_real_idx) {
            state.jsonb_detail.set_selected_line(visible_pos);
        }
    }
}

fn apply_pending_edit_as_draft(state: &mut AppState) {
    if !state.jsonb_detail.has_pending_changes() {
        return;
    }

    let content = state.jsonb_detail.editor().content().to_string();

    // Only apply valid JSON as draft
    if let Ok(compact) = serde_json::from_str::<serde_json::Value>(&content) {
        let compact_str = serde_json::to_string(&compact).unwrap_or_else(|_| content.clone());
        let row = state.jsonb_detail.row();
        let col = state.jsonb_detail.col();
        let original_cell = state
            .query
            .current_result()
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
        let mut state = AppState::new("test".to_string());
        state.query.set_current_result(Arc::new(QueryResult {
            query: String::new(),
            columns: vec!["id".to_string(), "settings".to_string()],
            rows: vec![vec![
                "1".to_string(),
                r#"{"theme":"dark","count":5}"#.to_string(),
            ]],
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
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(1); // settings (jsonb)
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
            state.result_interaction.enter_cell(0); // id (integer)

            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            assert!(!state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn blocked_on_null_cell() {
            let mut state = state_with_jsonb_cell();
            // Replace cell value with empty string (NULL)
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

        #[test]
        fn cursor_down_increments() {
            let mut state = state_with_jsonb_cell();
            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());
            assert_eq!(state.jsonb_detail.selected_line(), 0);

            reduce(&mut state, &Action::JsonbCursorDown, Instant::now());

            assert_eq!(state.jsonb_detail.selected_line(), 1);
        }

        #[test]
        fn cursor_up_decrements() {
            let mut state = state_with_jsonb_cell();
            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());
            reduce(&mut state, &Action::JsonbCursorDown, Instant::now());
            reduce(&mut state, &Action::JsonbCursorDown, Instant::now());

            reduce(&mut state, &Action::JsonbCursorUp, Instant::now());

            assert_eq!(state.jsonb_detail.selected_line(), 1);
        }

        #[test]
        fn toggle_fold_collapses_object() {
            let mut state = state_with_jsonb_cell();
            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());
            // Cursor at line 0 (root object open)

            reduce(&mut state, &Action::JsonbToggleFold, Instant::now());

            // After collapsing root, only 1 visible line
            assert_eq!(state.jsonb_detail.visible_count(), 1);
        }

        #[test]
        fn fold_all_collapses_everything() {
            let mut state = state_with_jsonb_cell();
            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            reduce(&mut state, &Action::JsonbFoldAll, Instant::now());

            assert_eq!(state.jsonb_detail.visible_count(), 1);
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
        fn yank_all_produces_clipboard_effect() {
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
        fn search_submit_deactivates_and_preserves_matches() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            // Type "theme" to get matches
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
        }

        #[test]
        fn text_input_updates_search_matches() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            // No matches initially
            assert!(state.jsonb_detail.search().matches.is_empty());

            // Type a search term that exists in the JSON
            for ch in "dark".chars() {
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
                "should find matches for 'dark'"
            );
        }

        #[test]
        fn search_next_cycles_through_matches() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            // Search for "t" — matches lines containing "count" and "theme"
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
        }

        #[test]
        fn search_prev_wraps_to_last_match() {
            let mut state = state_with_jsonb_cell();
            open_detail(&mut state);
            reduce(&mut state, &Action::JsonbEnterSearch, Instant::now());

            // Search for "t" — matches lines containing "count" and "theme"
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
            // current_match is 0, prev should wrap to last
            reduce(&mut state, &Action::JsonbSearchPrev, Instant::now());

            assert_eq!(state.jsonb_detail.search().current_match, match_count - 1);
        }
    }
}
