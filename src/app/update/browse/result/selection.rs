use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::update::helpers::EditGuardrailError;

use super::scroll::{result_col_count, result_row_count};

fn ensure_cell_visible(state: &mut AppState) {
    if let Some(col) = state.result_interaction.selection().cell() {
        let plan = state.ui.result_viewport_plan();
        let h_offset = state.result_interaction.horizontal_offset();
        if col < h_offset {
            state.result_interaction.set_horizontal_offset(col);
        } else if col >= h_offset + plan.column_count {
            // At max_offset every remaining column is visible, so clamping
            // never hides the active cell
            state.result_interaction.set_horizontal_offset(
                col.saturating_sub(plan.column_count.saturating_sub(1))
                    .min(plan.max_offset),
            );
        }
    }
}

pub(in crate::update) fn result_activate_cell(state: &mut AppState) -> Vec<Effect> {
    let rows = result_row_count(state);
    let cols = result_col_count(state);
    if rows > 0 && cols > 0 {
        let row = state.result_interaction.scroll_offset().min(rows - 1);
        let col = state.result_interaction.horizontal_offset().min(cols - 1);
        state.result_interaction.activate_cell(row, col);
    }
    vec![]
}

pub(in crate::update) fn result_exit_to_scroll(state: &mut AppState) -> Vec<Effect> {
    state.result_interaction.exit_cell_to_scroll();
    vec![]
}

pub(in crate::update) fn result_cell_left(state: &mut AppState) -> Vec<Effect> {
    if let Some(c) = state.result_interaction.selection().cell()
        && c > 0
    {
        state.result_interaction.move_cell(c - 1);
        ensure_cell_visible(state);
    }
    vec![]
}

pub(in crate::update) fn result_cell_right(state: &mut AppState) -> Vec<Effect> {
    if let Some(c) = state.result_interaction.selection().cell() {
        let max_col = result_col_count(state).saturating_sub(1);
        if c < max_col {
            state.result_interaction.move_cell(c + 1);
            ensure_cell_visible(state);
        }
    }
    vec![]
}

pub(in crate::update) fn result_delete_operator_pending(state: &mut AppState) -> Vec<Effect> {
    state.result_interaction.start_delete_operator();
    vec![]
}

pub(in crate::update) fn stage_row_for_delete(state: &mut AppState) -> Vec<Effect> {
    if state.session.is_read_only() {
        state
            .messages
            .set_error("Read-only mode: delete operations are disabled".to_string());
        return vec![];
    }
    if let Some(reason) = state.visible_preview_target_read_only_reason() {
        state
            .messages
            .set_error(EditGuardrailError::ReadOnlyPreviewTarget(reason).to_string());
        return vec![];
    }
    if let Some(row_idx) = state.result_interaction.selection().row() {
        state.result_interaction.stage_row(row_idx);
    }
    vec![]
}

pub(in crate::update) fn unstage_last_staged_row(state: &mut AppState) -> Vec<Effect> {
    state.result_interaction.unstage_last_row();
    vec![]
}

pub(in crate::update) fn clear_staged_deletes(state: &mut AppState) -> Vec<Effect> {
    state.result_interaction.clear_staged_deletes();
    vec![]
}

#[cfg(test)]
mod tests {
    use crate::domain::ColumnAttributes;

    use super::*;
    use crate::domain::Column;
    use crate::domain::{QueryResult, QuerySource, Table};
    use crate::services::AppServices;
    use crate::update::action::Action;
    use crate::update::reducer::route;
    use std::sync::Arc;
    use std::time::Instant;

    mod row_delete {
        use crate::test_support;

        use super::*;

        pub(super) fn base_state(
            pk: Option<Vec<&str>>,
            rows: Vec<Vec<&str>>,
            current_page: usize,
        ) -> AppState {
            let mut state = AppState::new("test".to_string());
            let _ = state.session.begin_connecting("postgres://localhost/test");
            state.session.set_selection_generation(7);
            state.query.pagination.reset_for_table("public", "users");
            state
                .query
                .pagination
                .set_page_result(current_page, state.query.pagination.reached_end());
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "SELECT * FROM public.users".to_string(),
                    vec!["id".to_string(), "name".to_string()],
                    rows.into_iter()
                        .map(|r| r.into_iter().map(ToString::to_string).collect())
                        .collect(),
                    1,
                    QuerySource::Preview,
                )));
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                columns: vec![Column {
                    attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                    ..test_support::column::test_nullable_column("id", "integer", 1)
                }],
                primary_key: pk.map(|cols| cols.into_iter().map(ToString::to_string).collect()),
                ..test_support::table::minimal("", "")
            }));
            state
        }

        #[test]
        fn dd_stages_active_row() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.activate_cell(0, 0);

            route(
                &mut state,
                Action::StageRowForDelete,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.result_interaction.staged_delete_rows().contains(&0));
        }

        #[test]
        fn dd_on_already_staged_row_is_noop() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.activate_cell(0, 0);
            state.result_interaction.stage_row(0);

            route(
                &mut state,
                Action::StageRowForDelete,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.result_interaction.staged_delete_rows().len(), 1);
        }

        #[test]
        fn staging_requires_active_cell() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);

            route(
                &mut state,
                Action::StageRowForDelete,
                Instant::now(),
                &AppServices::stub(),
            );

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

            route(
                &mut state,
                Action::UnstageLastStagedRow,
                Instant::now(),
                &AppServices::stub(),
            );

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

            route(
                &mut state,
                Action::ClearStagedDeletes,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }

        #[test]
        fn exit_to_scroll_preserves_staged_rows() {
            let mut state = base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.activate_cell(0, 0);
            state.result_interaction.stage_row(0);

            route(
                &mut state,
                Action::ResultExitToScroll,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.result_interaction.selection().row(), None);
            assert!(state.result_interaction.staged_delete_rows().contains(&0));
        }

        #[test]
        fn escape_from_result_scroll_clears_all_staged_rows() {
            let mut state = base_state(
                Some(vec!["id"]),
                vec![vec!["1", "alice"], vec!["2", "bob"]],
                0,
            );
            state.result_interaction.stage_row(0);
            state.result_interaction.stage_row(1);

            route(
                &mut state,
                Action::ClearStagedDeletes,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.result_interaction.staged_delete_rows().is_empty());
        }
    }

    mod read_only_guard {
        use crate::test_support;

        use super::*;

        #[test]
        fn read_only_blocks_stage_row_for_delete() {
            let mut state = row_delete::base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            state.result_interaction.activate_cell(0, 0);
            state.session.enable_read_only();

            let effects = route(
                &mut state,
                Action::StageRowForDelete,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.result_interaction.staged_delete_rows().is_empty());
            assert!(state.messages.last_error().is_some());
        }

        #[test]
        fn view_blocks_stage_row_for_delete() {
            let mut state = row_delete::base_state(Some(vec!["id"]), vec![vec!["1", "alice"]], 0);
            let mut table = state.session.table_detail().unwrap().clone();
            table.kind_info = test_support::table::view_kind_info();
            state.session.set_table_detail_raw(Some(table));
            state.result_interaction.activate_cell(0, 0);

            let effects = route(
                &mut state,
                Action::StageRowForDelete,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(effects.is_empty());
            assert!(state.result_interaction.staged_delete_rows().is_empty());
            assert_eq!(
                state.messages.last_error(),
                Some("Preview target is read-only: view")
            );
        }
    }
}
