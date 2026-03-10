use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;

use super::scroll::{result_col_count, result_row_count};

fn ensure_cell_visible(state: &mut AppState) {
    if let Some(col) = state.ui.result_selection.cell() {
        let plan = &state.ui.result_viewport_plan;
        let h_offset = state.ui.result_horizontal_offset;
        if col < h_offset {
            state.ui.result_horizontal_offset = col;
        } else if col >= h_offset + plan.column_count {
            state.ui.result_horizontal_offset =
                col.saturating_sub(plan.column_count.saturating_sub(1));
        }
    }
}

pub fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    match action {
        Action::ResultEnterRowActive => {
            let rows = result_row_count(state);
            if rows > 0 {
                let clamped = state.ui.result_scroll_offset.min(rows - 1);
                state.ui.result_selection.enter_row(clamped);
            }
            Some(vec![])
        }
        Action::ResultEnterCellActive => {
            if state.ui.result_selection.row().is_some() {
                state
                    .ui
                    .result_selection
                    .enter_cell(state.ui.result_horizontal_offset);
            }
            Some(vec![])
        }
        Action::ResultExitToRowActive => {
            state.ui.result_selection.exit_to_row();
            state.cell_edit.clear();
            state.pending_write_preview = None;
            Some(vec![])
        }
        Action::ResultExitToScroll => {
            state.ui.result_selection.reset();
            state.cell_edit.clear();
            state.ui.staged_delete_rows.clear();
            state.pending_write_preview = None;
            Some(vec![])
        }
        Action::ResultCellLeft => {
            if let Some(c) = state.ui.result_selection.cell()
                && c > 0
            {
                state.ui.result_selection.enter_cell(c - 1);
                ensure_cell_visible(state);
            }
            Some(vec![])
        }
        Action::ResultCellRight => {
            if let Some(c) = state.ui.result_selection.cell() {
                let max_col = result_col_count(state).saturating_sub(1);
                if c < max_col {
                    state.ui.result_selection.enter_cell(c + 1);
                    ensure_cell_visible(state);
                }
            }
            Some(vec![])
        }
        Action::ResultDeleteOperatorPending => {
            state.ui.delete_op_pending = true;
            Some(vec![])
        }
        Action::StageRowForDelete => {
            if state.ui.result_selection.mode() == crate::app::ui_state::ResultNavMode::RowActive
                && let Some(row_idx) = state.ui.result_selection.row()
            {
                state.ui.staged_delete_rows.insert(row_idx);
            }
            Some(vec![])
        }
        Action::UnstageLastStagedRow => {
            if let Some(&last) = state.ui.staged_delete_rows.iter().next_back() {
                state.ui.staged_delete_rows.remove(&last);
            }
            Some(vec![])
        }
        Action::ClearStagedDeletes => {
            state.ui.staged_delete_rows.clear();
            Some(vec![])
        }
        Action::ResultNextPage | Action::ResultPrevPage => {
            state.ui.result_selection.reset();
            state.cell_edit.clear();
            state.ui.staged_delete_rows.clear();
            state.pending_write_preview = None;
            None // Let the query reducer handle the actual page change
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, QueryResult, QuerySource, Table};
    use std::sync::Arc;
    use std::time::Instant;

    mod row_delete {
        use super::*;

        pub(super) fn base_state(
            pk: Option<Vec<&str>>,
            rows: Vec<Vec<&str>>,
            current_page: usize,
        ) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.cache.selection_generation = 7;
            state.query.pagination.current_page = current_page;
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.query.current_result = Some(Arc::new(QueryResult {
                query: "SELECT * FROM public.users".to_string(),
                columns: vec!["id".to_string(), "name".to_string()],
                row_count: rows.len(),
                rows: rows
                    .into_iter()
                    .map(|r| r.into_iter().map(|v| v.to_string()).collect())
                    .collect(),
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state.cache.table_detail = Some(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                }],
                primary_key: pk.map(|cols| cols.into_iter().map(|c| c.to_string()).collect()),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            });
            state
        }

        #[test]
        fn dd_stages_active_row() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.ui.result_selection.enter_row(0);

            reduce(&mut state, &Action::StageRowForDelete);

            assert!(state.ui.staged_delete_rows.contains(&0));
        }

        #[test]
        fn dd_on_already_staged_row_is_noop() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.ui.result_selection.enter_row(0);
            state.ui.staged_delete_rows.insert(0);

            reduce(&mut state, &Action::StageRowForDelete);

            assert_eq!(state.ui.staged_delete_rows.len(), 1);
        }

        #[test]
        fn staging_requires_row_active_mode() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);

            reduce(&mut state, &Action::StageRowForDelete);

            assert!(state.ui.staged_delete_rows.is_empty());
        }

        #[test]
        fn u_unstages_last_staged_row() {
            let mut state = base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.ui.staged_delete_rows.insert(0);
            state.ui.staged_delete_rows.insert(1);

            reduce(&mut state, &Action::UnstageLastStagedRow);

            assert_eq!(state.ui.staged_delete_rows.len(), 1);
            assert!(state.ui.staged_delete_rows.contains(&0));
        }

        #[test]
        fn clear_staged_deletes_removes_all() {
            let mut state = base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.ui.staged_delete_rows.insert(0);
            state.ui.staged_delete_rows.insert(1);

            reduce(&mut state, &Action::ClearStagedDeletes);

            assert!(state.ui.staged_delete_rows.is_empty());
        }
    }

    mod page_passthrough {
        use super::*;
        use crate::app::write_guardrails::{
            GuardrailDecision, RiskLevel, TargetSummary, WriteOperation, WritePreview,
        };

        fn dirty_state() -> AppState {
            let mut state = row_delete::base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.ui.result_selection.enter_row(0);
            state.ui.result_selection.enter_cell(1);
            state.cell_edit.begin(0, 1, "alice".to_string());
            state.ui.staged_delete_rows.insert(1);
            state.pending_write_preview = Some(WritePreview {
                operation: WriteOperation::Update,
                sql: String::new(),
                target_summary: TargetSummary {
                    schema: "public".into(),
                    table: "users".into(),
                    key_values: vec![],
                },
                diff: vec![],
                guardrail: GuardrailDecision {
                    risk_level: RiskLevel::Low,
                    blocked: false,
                    reason: None,
                    target_summary: None,
                },
            });
            state
        }

        #[test]
        fn next_page_resets_all_view_state_and_returns_none() {
            let mut state = dirty_state();

            let result = reduce(&mut state, &Action::ResultNextPage);

            assert!(result.is_none());
            assert!(state.ui.result_selection.row().is_none());
            assert!(state.ui.result_selection.cell().is_none());
            assert!(!state.cell_edit.is_active());
            assert!(state.ui.staged_delete_rows.is_empty());
            assert!(state.pending_write_preview.is_none());
        }

        #[test]
        fn prev_page_resets_all_view_state_and_returns_none() {
            let mut state = dirty_state();

            let result = reduce(&mut state, &Action::ResultPrevPage);

            assert!(result.is_none());
            assert!(state.ui.result_selection.row().is_none());
            assert!(!state.cell_edit.is_active());
            assert!(state.ui.staged_delete_rows.is_empty());
            assert!(state.pending_write_preview.is_none());
        }
    }
}
