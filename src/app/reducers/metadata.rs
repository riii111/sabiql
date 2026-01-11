//! Metadata sub-reducer: metadata loading, table detail, and prefetch.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::connection_error::ConnectionErrorInfo;
use crate::app::connection_state::ConnectionState;
use crate::app::effect::Effect;
use crate::app::er_state::ErStatus;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;
use crate::domain::MetadataState;

/// Handles metadata loading, table detail, and prefetch actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_metadata(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::MetadataLoaded(metadata) => {
            let has_tables = !metadata.tables.is_empty();
            state.cache.metadata = Some(*metadata.clone());
            state.cache.state = MetadataState::Loaded;
            state.runtime.connection_state = ConnectionState::Connected;
            state
                .ui
                .set_explorer_selection(if has_tables { Some(0) } else { None });

            let mut effects = vec![];

            state.connection_error.clear();

            if state.runtime.is_reloading {
                state.messages.set_success_at("Reloaded!".to_string(), now);
                state.runtime.is_reloading = false;
            }

            if state.ui.input_mode == InputMode::SqlModal && !state.sql_modal.prefetch_started {
                effects.push(Effect::DispatchActions(vec![Action::StartPrefetchAll]));
            }

            Some(effects)
        }
        Action::MetadataFailed(error) => {
            let error_info = ConnectionErrorInfo::new(error);
            state.connection_error.set_error(error_info);
            state.cache.state = MetadataState::Error(error.clone());
            state.runtime.is_reloading = false;
            if !state.runtime.connection_state.is_connected() {
                state.runtime.connection_state = ConnectionState::Failed;
                state.ui.input_mode = InputMode::ConnectionError;
            }
            Some(vec![])
        }
        Action::TableDetailLoaded(detail, generation) => {
            if *generation == state.cache.selection_generation {
                state.cache.table_detail = Some(*detail.clone());
                state.ui.inspector_scroll_offset = 0;
            }
            Some(vec![])
        }
        Action::TableDetailFailed(error, generation) => {
            if *generation == state.cache.selection_generation {
                state.set_error(error.clone());
            }
            Some(vec![])
        }

        Action::LoadMetadata => {
            if let Some(dsn) = &state.runtime.dsn {
                state.cache.state = MetadataState::Loading;
                Some(vec![Effect::FetchMetadata { dsn: dsn.clone() }])
            } else {
                Some(vec![])
            }
        }
        Action::LoadTableDetail {
            schema,
            table,
            generation,
        } => {
            if let Some(dsn) = &state.runtime.dsn {
                Some(vec![Effect::FetchTableDetail {
                    dsn: dsn.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                    generation: *generation,
                }])
            } else {
                Some(vec![])
            }
        }

        Action::ReloadMetadata => {
            if let Some(dsn) = &state.runtime.dsn {
                state.runtime.is_reloading = true;
                state.sql_modal.prefetch_started = false;
                state.sql_modal.prefetch_queue.clear();
                state.sql_modal.prefetching_tables.clear();
                state.sql_modal.failed_prefetch_tables.clear();
                state.er_preparation.reset();
                state.messages.last_error = None;
                state.messages.last_success = None;
                state.messages.expires_at = None;

                Some(vec![Effect::Sequence(vec![
                    Effect::CacheInvalidate { dsn: dsn.clone() },
                    Effect::ClearCompletionEngineCache,
                    Effect::FetchMetadata { dsn: dsn.clone() },
                ])])
            } else {
                Some(vec![])
            }
        }

        Action::StartPrefetchAll => {
            if !state.sql_modal.prefetch_started
                && let Some(metadata) = &state.cache.metadata
            {
                state.sql_modal.prefetch_started = true;
                state.sql_modal.prefetch_queue.clear();
                state.er_preparation.pending_tables.clear();
                state.er_preparation.fetching_tables.clear();
                state.er_preparation.failed_tables.clear();
                state.er_preparation.total_tables = metadata.tables.len();

                for table_summary in &metadata.tables {
                    let qualified_name = table_summary.qualified_name();
                    state
                        .sql_modal
                        .prefetch_queue
                        .push_back(qualified_name.clone());
                    state.er_preparation.pending_tables.insert(qualified_name);
                }
                Some(vec![Effect::ProcessPrefetchQueue])
            } else {
                Some(vec![])
            }
        }

        Action::ProcessPrefetchQueue => {
            const MAX_CONCURRENT_PREFETCH: usize = 4;
            let current_in_flight = state.sql_modal.prefetching_tables.len();
            let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);

            let mut actions = Vec::new();
            for _ in 0..available_slots {
                if let Some(qualified_name) = state.sql_modal.prefetch_queue.pop_front()
                    && let Some((schema, table)) = qualified_name.split_once('.')
                {
                    actions.push(Action::PrefetchTableDetail {
                        schema: schema.to_string(),
                        table: table.to_string(),
                    });
                }
            }

            if actions.is_empty() {
                Some(vec![])
            } else {
                Some(vec![Effect::DispatchActions(actions)])
            }
        }

        Action::PrefetchTableDetail { schema, table } => {
            let qualified_name = format!("{}.{}", schema, table);

            if state.sql_modal.prefetching_tables.contains(&qualified_name) {
                return Some(vec![]);
            }

            const PREFETCH_BACKOFF_SECS: u64 = 30;
            let recently_failed = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified_name)
                .map(|(t, _): &(Instant, String)| t.elapsed().as_secs() < PREFETCH_BACKOFF_SECS)
                .unwrap_or(false);

            if recently_failed {
                return Some(vec![]);
            }

            state
                .sql_modal
                .prefetching_tables
                .insert(qualified_name.clone());
            state.er_preparation.pending_tables.remove(&qualified_name);
            state
                .er_preparation
                .fetching_tables
                .insert(qualified_name.clone());

            if let Some(dsn) = &state.runtime.dsn {
                Some(vec![Effect::PrefetchTableDetail {
                    dsn: dsn.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                }])
            } else {
                Some(vec![])
            }
        }

        Action::TableDetailCached {
            schema,
            table,
            detail,
        } => {
            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);

            let mut effects = vec![Effect::CacheTableInCompletionEngine {
                qualified_name,
                table: detail.clone(),
            }];

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                if !state.er_preparation.has_failures() {
                    state.set_success("ER ready. Press 'e' to open.".to_string());
                } else {
                    let failed_count = state.er_preparation.failed_tables.len();
                    let failed_data: Vec<(String, String)> = state
                        .er_preparation
                        .failed_tables
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    effects.push(Effect::WriteErFailureLog {
                        failed_tables: failed_data,
                    });
                    state.set_error(format!(
                        "ER failed: {} table(s) failed. 'e' to retry.",
                        failed_count
                    ));
                }
            }

            Some(effects)
        }

        Action::TableDetailCacheFailed {
            schema,
            table,
            error,
        } => {
            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .insert(qualified_name.clone(), (now, error.clone()));
            state
                .er_preparation
                .on_table_failed(&qualified_name, error.clone());

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                let failed_count = state.er_preparation.failed_tables.len();
                let failed_data: Vec<(String, String)> = state
                    .er_preparation
                    .failed_tables
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                effects.push(Effect::WriteErFailureLog {
                    failed_tables: failed_data,
                });
                state.set_error(format!(
                    "ER failed: {} table(s) failed. See log for details. 'e' to retry.",
                    failed_count
                ));
            }

            Some(effects)
        }

        Action::TableDetailAlreadyCached { schema, table } => {
            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                if !state.er_preparation.has_failures() {
                    state.set_success("ER ready. Press 'e' to open.".to_string());
                } else {
                    let failed_count = state.er_preparation.failed_tables.len();
                    let failed_data: Vec<(String, String)> = state
                        .er_preparation
                        .failed_tables
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    effects.push(Effect::WriteErFailureLog {
                        failed_tables: failed_data,
                    });
                    state.set_error(format!(
                        "ER failed: {} table(s) failed. 'e' to retry.",
                        failed_count
                    ));
                }
            }

            Some(effects)
        }

        _ => None,
    }
}
