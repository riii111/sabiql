use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::browse::jsonb_detail::JsonbDetailState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::policy::json::{parse_json_tree, visible_line_indices};
use crate::app::policy::write::write_guardrails::{
    ColumnDiff, TargetSummary, WriteOperation, WritePreview, evaluate_guardrails,
};
use crate::app::policy::write::write_update::build_pk_pairs;
use crate::app::services::AppServices;
use crate::app::update::action::{Action, InputTarget};
use crate::app::update::helpers::editable_preview_base;
use crate::domain::QuerySource;

pub fn reduce(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> Option<Vec<Effect>> {
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
            if let Some(&real_idx) = state.jsonb_detail.visible_indices().get(selected) {
                state.jsonb_detail.tree_mut().toggle_fold(real_idx);
                post_fold_fixup(state);
            }
            Some(vec![])
        }

        Action::JsonbFoldAll => {
            state.jsonb_detail.tree_mut().fold_all();
            post_fold_fixup(state);
            Some(vec![])
        }

        Action::JsonbUnfoldAll => {
            state.jsonb_detail.tree_mut().unfold_all();
            post_fold_fixup(state);
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

        // ── Edit lifecycle ──────────────────────────────────────────
        Action::JsonbEnterEdit => {
            if state.session.read_only {
                state
                    .messages
                    .set_error_at("Read-only mode: editing is disabled".to_string(), now);
                return Some(vec![]);
            }
            let pretty = pretty_print_json(state.jsonb_detail.original_json());
            // Map visible selection to a line in the pretty-printed JSON.
            // Tree lines and pretty-printed lines have 1:1 correspondence.
            let visible = visible_line_indices(state.jsonb_detail.tree());
            let target_line = visible
                .get(state.jsonb_detail.selected_line())
                .copied()
                .unwrap_or(0);
            state.jsonb_detail.enter_edit(pretty, target_line);
            state.modal.replace_mode(InputMode::JsonbEdit);
            Some(vec![])
        }

        Action::JsonbExitEdit => {
            state.jsonb_detail.exit_edit();
            state.modal.replace_mode(InputMode::JsonbDetail);
            Some(vec![])
        }

        Action::JsonbSubmitEdit => match state.jsonb_detail.validate_editor_content() {
            Ok(json_content) => match build_jsonb_update_preview(state, &json_content, services) {
                Ok(preview) => Some(vec![Effect::DispatchActions(vec![
                    Action::OpenWritePreviewConfirm(Box::new(preview)),
                ])]),
                Err(msg) => {
                    state.messages.set_error_at(msg, now);
                    Some(vec![])
                }
            },
            Err(msg) => {
                state.messages.set_error_at(msg, now);
                Some(vec![])
            }
        },

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

fn post_fold_fixup(state: &mut AppState) {
    state.jsonb_detail.rebuild_visible_indices();
    let vc = state.jsonb_detail.visible_count();
    state.jsonb_detail.clamp_cursor(vc);
    state.jsonb_detail.clamp_scroll(vc);
    if state.jsonb_detail.search().active {
        update_search_matches(state);
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

fn pretty_print_json(json_str: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json_str)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| json_str.to_string())
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

fn build_jsonb_update_preview(
    state: &AppState,
    new_json: &str,
    services: &AppServices,
) -> Result<WritePreview, String> {
    let (result, pk_cols) = editable_preview_base(state)?;
    let row_idx = state.jsonb_detail.row();

    let row = result
        .rows
        .get(row_idx)
        .ok_or_else(|| "Row index out of bounds".to_string())?;
    let column_name = state.jsonb_detail.column_name().to_string();

    let pk_pairs = build_pk_pairs(&result.columns, row, pk_cols);
    let target = TargetSummary {
        schema: state.query.pagination.schema.clone(),
        table: state.query.pagination.table.clone(),
        key_values: pk_pairs.clone().unwrap_or_default(),
    };
    let has_where = pk_pairs.as_ref().is_some_and(|p| !p.is_empty());
    let has_stable_row_identity = pk_pairs.is_some();
    let guardrail = evaluate_guardrails(has_where, has_stable_row_identity, Some(target.clone()));
    if guardrail.blocked {
        return Err(guardrail
            .reason
            .unwrap_or_else(|| "Write blocked by guardrails".to_string()));
    }

    let sql = services.sql_dialect.build_update_sql(
        &target.schema,
        &target.table,
        &column_name,
        new_json,
        &target.key_values,
    );

    Ok(WritePreview {
        operation: WriteOperation::Update,
        sql,
        target_summary: target,
        diff: vec![ColumnDiff {
            column: column_name,
            before: state.jsonb_detail.original_json().to_string(),
            after: new_json.to_string(),
        }],
        guardrail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::AppServices;
    use crate::domain::column::Column;
    use crate::domain::{QueryResult, QuerySource, Table};
    use std::sync::Arc;

    fn stub() -> AppServices {
        AppServices::stub()
    }

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

            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );

            assert!(state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::JsonbDetail);
        }

        #[test]
        fn blocked_on_non_jsonb_column() {
            let mut state = state_with_jsonb_cell();
            state.result_interaction.enter_cell(0); // id (integer)

            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );

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

            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );

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

            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );

            assert!(!state.jsonb_detail.is_active());
        }

        #[test]
        fn blocked_without_table_detail() {
            let mut state = state_with_jsonb_cell();
            state.session.set_table_detail_raw(None);

            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );

            assert!(!state.jsonb_detail.is_active());
        }
    }

    mod navigation {
        use super::*;

        #[test]
        fn close_clears_state() {
            let mut state = state_with_jsonb_cell();
            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );
            assert!(state.jsonb_detail.is_active());

            reduce(
                &mut state,
                &Action::CloseJsonbDetail,
                Instant::now(),
                &stub(),
            );

            assert!(!state.jsonb_detail.is_active());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn cursor_down_increments() {
            let mut state = state_with_jsonb_cell();
            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );
            assert_eq!(state.jsonb_detail.selected_line(), 0);

            reduce(
                &mut state,
                &Action::JsonbCursorDown,
                Instant::now(),
                &stub(),
            );

            assert_eq!(state.jsonb_detail.selected_line(), 1);
        }

        #[test]
        fn cursor_up_decrements() {
            let mut state = state_with_jsonb_cell();
            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );
            reduce(
                &mut state,
                &Action::JsonbCursorDown,
                Instant::now(),
                &stub(),
            );
            reduce(
                &mut state,
                &Action::JsonbCursorDown,
                Instant::now(),
                &stub(),
            );

            reduce(&mut state, &Action::JsonbCursorUp, Instant::now(), &stub());

            assert_eq!(state.jsonb_detail.selected_line(), 1);
        }

        #[test]
        fn toggle_fold_collapses_object() {
            let mut state = state_with_jsonb_cell();
            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );
            // Cursor at line 0 (root object open)

            reduce(
                &mut state,
                &Action::JsonbToggleFold,
                Instant::now(),
                &stub(),
            );

            // After collapsing root, only 1 visible line
            assert_eq!(state.jsonb_detail.visible_count(), 1);
        }

        #[test]
        fn fold_all_collapses_everything() {
            let mut state = state_with_jsonb_cell();
            reduce(
                &mut state,
                &Action::OpenJsonbDetail,
                Instant::now(),
                &stub(),
            );

            reduce(&mut state, &Action::JsonbFoldAll, Instant::now(), &stub());

            assert_eq!(state.jsonb_detail.visible_count(), 1);
        }
    }
}
