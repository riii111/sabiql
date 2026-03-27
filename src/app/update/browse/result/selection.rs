use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::update::action::Action;

use super::scroll::{result_col_count, result_row_count};

fn ensure_cell_visible(state: &mut AppState) {
    if let Some(col) = state.result_interaction.selection().cell() {
        let plan = &state.ui.result_viewport_plan;
        let h_offset = state.result_interaction.horizontal_offset;
        if col < h_offset {
            state.result_interaction.horizontal_offset = col;
        } else if col >= h_offset + plan.column_count {
            state.result_interaction.horizontal_offset =
                col.saturating_sub(plan.column_count.saturating_sub(1));
        }
    }
}

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ResultEnterRowActive => {
            let rows = result_row_count(state);
            if rows > 0 {
                let clamped = state.result_interaction.scroll_offset.min(rows - 1);
                state.result_interaction.enter_row(clamped);
            }
            Some(vec![])
        }
        Action::ResultEnterCellActive => {
            if state.result_interaction.selection().row().is_some() {
                state
                    .result_interaction
                    .enter_cell(state.result_interaction.horizontal_offset);
            }
            Some(vec![])
        }
        Action::ResultExitToRowActive => {
            state.result_interaction.exit_cell_to_row();
            Some(vec![])
        }
        Action::ResultExitToScroll => {
            state.result_interaction.exit_row_to_scroll();
            Some(vec![])
        }
        Action::ResultCellLeft => {
            if let Some(c) = state.result_interaction.selection().cell()
                && c > 0
            {
                state.result_interaction.enter_cell(c - 1);
                ensure_cell_visible(state);
            }
            Some(vec![])
        }
        Action::ResultCellRight => {
            if let Some(c) = state.result_interaction.selection().cell() {
                let max_col = result_col_count(state).saturating_sub(1);
                if c < max_col {
                    state.result_interaction.enter_cell(c + 1);
                    ensure_cell_visible(state);
                }
            }
            Some(vec![])
        }
        Action::ResultDeleteOperatorPending => {
            state.result_interaction.delete_op_pending = true;
            Some(vec![])
        }
        Action::StageRowForDelete => {
            if state.session.read_only {
                state.messages.set_error_at(
                    "Read-only mode: delete operations are disabled".to_string(),
                    now,
                );
                return Some(vec![]);
            }
            if state.result_interaction.selection().mode()
                == crate::app::model::shared::ui_state::ResultNavMode::RowActive
                && let Some(row_idx) = state.result_interaction.selection().row()
            {
                state.result_interaction.stage_row(row_idx);
            }
            Some(vec![])
        }
        Action::UnstageLastStagedRow => {
            state.result_interaction.unstage_last_row();
            Some(vec![])
        }
        Action::ClearStagedDeletes => {
            state.result_interaction.clear_staged_deletes();
            Some(vec![])
        }
        Action::ResultNextPage | Action::ResultPrevPage => {
            None // Handled entirely by the query reducer (reset only after transition confirmed)
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
            state.session.dsn = Some("postgres://localhost/test".to_string());
            state.session.set_selection_generation(7);
            state.query.pagination.current_page = current_page;
            state.query.pagination.schema = "public".to_string();
            state.query.pagination.table = "users".to_string();
            state.query.set_current_result(Arc::new(QueryResult {
                query: "SELECT * FROM public.users".to_string(),
                columns: vec!["id".to_string(), "name".to_string()],
                row_count: rows.len(),
                rows: rows
                    .into_iter()
                    .map(|r| r.into_iter().map(ToString::to_string).collect())
                    .collect(),
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: QuerySource::Preview,
                error: None,
                command_tag: None,
            }));
            state.session.set_table_detail_raw(Some(Table {
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
                primary_key: pk.map(|cols| cols.into_iter().map(ToString::to_string).collect()),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }));
            state
        }

        #[test]
        fn dd_stages_active_row() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.enter_row(0);

            reduce(&mut state, &Action::StageRowForDelete, Instant::now());

            assert!(state.result_interaction.staged_delete_rows().contains(&0));
        }

        #[test]
        fn dd_on_already_staged_row_is_noop() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.enter_row(0);
            state.result_interaction.stage_row(0);

            reduce(&mut state, &Action::StageRowForDelete, Instant::now());

            assert_eq!(state.result_interaction.staged_delete_rows().len(), 1);
        }

        #[test]
        fn staging_requires_row_active_mode() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);

            reduce(&mut state, &Action::StageRowForDelete, Instant::now());

            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }

        #[test]
        fn u_unstages_last_staged_row() {
            let mut state = base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.result_interaction.stage_row(0);
            state.result_interaction.stage_row(1);

            reduce(&mut state, &Action::UnstageLastStagedRow, Instant::now());

            assert_eq!(state.result_interaction.staged_delete_rows().len(), 1);
            assert!(state.result_interaction.staged_delete_rows().contains(&0));
        }

        #[test]
        fn clear_staged_deletes_removes_all() {
            let mut state = base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.result_interaction.stage_row(0);
            state.result_interaction.stage_row(1);

            reduce(&mut state, &Action::ClearStagedDeletes, Instant::now());

            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }
    }

    mod read_only_guard {
        use super::*;

        #[test]
        fn read_only_blocks_stage_row_for_delete() {
            let mut state = row_delete::base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.enter_row(0);
            state.session.read_only = true;

            let effects = reduce(&mut state, &Action::StageRowForDelete, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert!(state.result_interaction.staged_delete_rows().is_empty());
            assert!(state.messages.last_error.is_some());
        }
    }

    mod page_passthrough {
        use super::*;

        #[test]
        fn next_page_returns_none_without_mutating_state() {
            let mut state = row_delete::base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.enter_row(0);
            state.result_interaction.stage_row(0);

            let result = reduce(&mut state, &Action::ResultNextPage, Instant::now());

            assert!(result.is_none());
            assert_eq!(state.result_interaction.selection().row(), Some(0));
            assert!(state.result_interaction.staged_delete_rows().contains(&0));
        }

        #[test]
        fn prev_page_returns_none_without_mutating_state() {
            let mut state = row_delete::base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.enter_row(0);
            state.result_interaction.stage_row(0);

            let result = reduce(&mut state, &Action::ResultPrevPage, Instant::now());

            assert!(result.is_none());
            assert_eq!(state.result_interaction.selection().row(), Some(0));
            assert!(state.result_interaction.staged_delete_rows().contains(&0));
        }
    }
}
