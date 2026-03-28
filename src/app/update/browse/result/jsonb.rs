use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::policy::json::{parse_json_tree, visible_line_indices};
use crate::app::update::action::Action;
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
            state.jsonb_detail.close();
            state.modal.pop_mode();
            Some(vec![])
        }

        Action::JsonbCursorUp => {
            let visible_count = visible_line_indices(state.jsonb_detail.tree()).len();
            state.jsonb_detail.cursor_up(visible_count);
            Some(vec![])
        }

        Action::JsonbCursorDown => {
            let visible_count = visible_line_indices(state.jsonb_detail.tree()).len();
            state.jsonb_detail.cursor_down(visible_count);
            Some(vec![])
        }

        Action::JsonbScrollToTop => {
            state.jsonb_detail.cursor_to_top();
            Some(vec![])
        }

        Action::JsonbScrollToEnd => {
            let visible_count = visible_line_indices(state.jsonb_detail.tree()).len();
            state.jsonb_detail.cursor_to_end(visible_count);
            Some(vec![])
        }

        Action::JsonbToggleFold => {
            let visible = visible_line_indices(state.jsonb_detail.tree());
            if let Some(&real_idx) = visible.get(state.jsonb_detail.selected_line()) {
                state.jsonb_detail.tree_mut().toggle_fold(real_idx);
                let new_visible_count = visible_line_indices(state.jsonb_detail.tree()).len();
                state.jsonb_detail.clamp_cursor(new_visible_count);
            }
            Some(vec![])
        }

        Action::JsonbFoldAll => {
            state.jsonb_detail.tree_mut().fold_all();
            let visible_count = visible_line_indices(state.jsonb_detail.tree()).len();
            state.jsonb_detail.clamp_cursor(visible_count);
            Some(vec![])
        }

        Action::JsonbUnfoldAll => {
            state.jsonb_detail.tree_mut().unfold_all();
            Some(vec![])
        }

        Action::JsonbYankAll => {
            let json = state.jsonb_detail.original_json().to_string();
            Some(vec![Effect::CopyToClipboard {
                content: json,
                on_success: Some(Action::CellCopied),
                on_failure: Some(Action::CopyFailed(crate::app::ports::ClipboardError {
                    message: "Clipboard unavailable".into(),
                })),
            }])
        }

        _ => None,
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
            let visible = visible_line_indices(state.jsonb_detail.tree());
            assert_eq!(visible.len(), 1);
        }

        #[test]
        fn fold_all_collapses_everything() {
            let mut state = state_with_jsonb_cell();
            reduce(&mut state, &Action::OpenJsonbDetail, Instant::now());

            reduce(&mut state, &Action::JsonbFoldAll, Instant::now());

            let visible = visible_line_indices(state.jsonb_detail.tree());
            assert_eq!(visible.len(), 1);
        }
    }
}
