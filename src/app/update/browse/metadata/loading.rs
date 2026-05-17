use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::browse::query_execution::PREVIEW_PAGE_SIZE;
use crate::model::connection::error::ConnectionErrorInfo;
use crate::model::er_state::ErStatus;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ModalKind};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_loading(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::MetadataLoaded {
            dsn,
            run_id,
            metadata,
        } => {
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.session.is_current_metadata_run(*run_id)
            {
                return DispatchResult::handled();
            }

            let has_tables = !metadata.table_summaries.is_empty();
            state.session.mark_connected(Arc::clone(metadata));

            let mut effects = vec![];

            if state.query.pagination.table.is_empty() {
                state
                    .ui
                    .set_explorer_selection(if has_tables { Some(0) } else { None });
            } else {
                let prev_schema = &state.query.pagination.schema;
                let prev_table = &state.query.pagination.table;
                let found_index = metadata
                    .table_summaries
                    .iter()
                    .position(|t| &t.schema == prev_schema && &t.name == prev_table);
                if let Some(idx) = found_index {
                    state.ui.set_explorer_selection(Some(idx));
                    // Refresh preview and detail: DDL or reload may have changed
                    // data/schema even though the table still exists.
                    let dsn = dsn.clone();
                    let page = state.query.pagination.current_page;
                    let generation = state.session.selection_generation();
                    let query_run_id = state.query.begin_running(now);
                    let detail_run_id = state.session.begin_table_detail_run();
                    effects.push(Effect::ExecutePreview {
                        dsn: dsn.clone(),
                        schema: state.query.pagination.schema.clone(),
                        table: state.query.pagination.table.clone(),
                        generation,
                        run_id: query_run_id,
                        limit: PREVIEW_PAGE_SIZE,
                        offset: page * PREVIEW_PAGE_SIZE,
                        target_page: page,
                        read_only: state.session.read_only,
                    });
                    effects.push(Effect::FetchTableDetail {
                        dsn,
                        schema: state.query.pagination.schema.clone(),
                        table: state.query.pagination.table.clone(),
                        generation,
                        run_id: detail_run_id,
                    });
                } else {
                    // The previously selected table was removed (e.g. via DROP TABLE).
                    // Clear all selection state to avoid stale references.
                    state
                        .ui
                        .set_explorer_selection(if has_tables { Some(0) } else { None });
                    state
                        .session
                        .clear_table_selection(&mut state.query.pagination);
                    state.query.clear_current_result();
                }
            }

            state.connection_error.clear();

            if state.session.is_reloading {
                state.messages.set_success_at("Reloaded!".to_string(), now);
                state.session.finish_reload();
            }

            if state.modal.active_mode() == InputMode::SqlModal
                && !state.sql_modal.is_prefetch_started()
            {
                effects.push(Effect::DispatchActions(vec![Action::StartPrefetchAll]));
            }

            if state.ui.take_pending_er_picker() && state.modal.active_mode() == InputMode::Normal {
                effects.push(Effect::DispatchActions(vec![Action::OpenModal(
                    ModalKind::ErTablePicker,
                )]));
            }

            DispatchResult::handled_with(effects)
        }
        Action::MetadataFailed { dsn, run_id, error } => {
            if state.session.dsn.as_ref() != Some(dsn)
                || !state.session.is_current_metadata_run(*run_id)
            {
                return DispatchResult::handled();
            }

            let error_info = ConnectionErrorInfo::from_db_operation_error(error);
            state.connection_error.set_error(error_info);
            let was_connected = state.session.connection_state().is_connected();
            state.session.mark_connection_failed(error.masked_details());
            if !was_connected {
                state.modal.replace_mode(InputMode::ConnectionError);
            }
            if state.er_preparation.status == ErStatus::Waiting {
                state.er_preparation.mark_idle();
            }
            DispatchResult::handled()
        }
        Action::LoadMetadata => {
            if let Some(dsn) = state.session.dsn.clone() {
                let run_id = state.session.begin_metadata_refresh();
                DispatchResult::handled_with(vec![Effect::FetchMetadata { dsn, run_id }])
            } else {
                DispatchResult::handled()
            }
        }
        Action::ReloadMetadata => {
            if let Some(dsn) = state.session.dsn.clone() {
                let run_id = state.session.begin_reload();
                state.sql_modal.reset_prefetch();
                state.er_preparation.reset();
                state.ui.reset_er_picker_request();
                state.messages.clear();

                DispatchResult::handled_with(vec![Effect::Sequence(vec![
                    Effect::CacheInvalidate { dsn: dsn.clone() },
                    Effect::ClearCompletionEngineCache,
                    Effect::FetchMetadata { dsn, run_id },
                ])])
            } else {
                DispatchResult::handled()
            }
        }
        _ => DispatchResult::pass(),
    }
}
