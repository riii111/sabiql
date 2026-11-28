use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{Table, TableSummary};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::DbOperationError;
use crate::update::helpers::reject_pending_mysql_connection_probe;
use crate::update::sql_editor::completion::completion_request;

use super::check_er_completion;

fn start_batch(state: &mut AppState, tables: Vec<String>, tracks_er: bool) -> Vec<Effect> {
    let Some(endpoint) = state.session.endpoint().cloned() else {
        return vec![];
    };
    let mut names: Vec<String> = tables
        .into_iter()
        .filter(|name| !state.table_detail_cache.contains(name))
        .filter(|name| !state.table_prefetch.failed_prefetch(name))
        .collect();
    names.sort();
    names.dedup();
    let mut requested = Vec::with_capacity(names.len());
    for name in &names {
        if let Some((schema, table)) = name.split_once('.') {
            requested.push((schema.to_string(), table.to_string()));
        } else {
            state
                .table_prefetch
                .fail(name.clone(), "Invalid table name".to_string());
        }
    }
    let valid_names = requested.iter().map(|(s, t)| format!("{s}.{t}"));
    let run_id = state.table_prefetch.begin(valid_names, tracks_er);
    if requested.is_empty() {
        return if tracks_er {
            check_er_completion(state)
        } else {
            state.table_prefetch.invalidate_prefetch();
            vec![]
        };
    }
    vec![Effect::PrefetchTableColumnsAndFks {
        endpoint,
        run_id,
        tables: requested,
    }]
}

pub(in crate::update) fn start_er_prefetch_all(state: &mut AppState) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state)
        || (state.table_prefetch.active_prefetch_run_id().is_some()
            && state.table_prefetch.prefetch_tracks_er())
    {
        return vec![];
    }
    let Some(metadata) = state.session.metadata() else {
        return vec![];
    };
    let names: Vec<String> = metadata
        .table_summaries
        .iter()
        .map(TableSummary::qualified_name)
        .collect();
    state.table_detail_cache.resize_for_table_count(names.len());
    state
        .er_preparation
        .begin_all_prefetch(names.iter().cloned());
    state.table_prefetch.clear_failures();
    start_batch(state, names, true)
}

pub(in crate::update) fn start_er_prefetch_scoped(
    state: &mut AppState,
    tables: Vec<String>,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state)
        || (state.table_prefetch.active_prefetch_run_id().is_some()
            && state.table_prefetch.prefetch_tracks_er())
    {
        return vec![];
    }
    state.er_preparation.begin_scoped_prefetch(&tables);
    if let Some(metadata) = state.session.metadata() {
        state
            .table_detail_cache
            .resize_for_table_count(metadata.table_summaries.len());
    }
    state.table_prefetch.clear_failures();
    start_batch(state, tables, true)
}

pub(super) fn start_er_neighbor_prefetch(state: &mut AppState, tables: Vec<String>) -> Vec<Effect> {
    start_batch(state, tables, true)
}

pub(in crate::update) fn start_completion_prefetch(
    state: &mut AppState,
    mut tables: Vec<String>,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state)
        || tables.is_empty()
        || (state.table_prefetch.active_prefetch_run_id().is_some()
            && state.table_prefetch.prefetch_tracks_er())
    {
        return vec![];
    }
    tables.retain(|name| {
        !state.table_detail_cache.contains(name) && !state.table_prefetch.failed_prefetch(name)
    });
    if tables
        .iter()
        .all(|name| state.table_prefetch.in_flight().contains(name))
    {
        return vec![];
    }
    tables.extend(state.table_prefetch.in_flight().iter().cloned());
    start_batch(state, tables, false)
}

pub(in crate::update) fn table_details_cached(
    state: &mut AppState,
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
    outcome: Result<Vec<Table>, DbOperationError>,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state)
        || !state.session.endpoint_matches(&endpoint)
        || !state.table_prefetch.is_current_prefetch_run(run_id)
    {
        return vec![];
    }
    let mut remaining = state.table_prefetch.finish();
    match outcome {
        Ok(details) => {
            for detail in details {
                let name = detail.qualified_name();
                if remaining.remove(&name) {
                    state.table_detail_cache.insert(name, detail);
                }
            }
            for name in remaining {
                state
                    .table_prefetch
                    .fail(name, "Table not found".to_string());
            }
        }
        Err(error) => {
            let error = error.user_message();
            for name in remaining {
                state.table_prefetch.fail(name, error.clone());
            }
        }
    }
    if state.table_prefetch.prefetch_tracks_er() {
        check_er_completion(state)
    } else {
        state.table_prefetch.invalidate_prefetch();
        if state.modal.active_mode() == InputMode::SqlModal {
            completion_request(state, now)
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConnectionId, DatabaseMetadata};
    use crate::test_support::{endpoint, table};

    fn state() -> AppState {
        let mut state = AppState::new("test".into());
        state.session.activate_connection(
            &ConnectionId::new(),
            "test",
            &endpoint::postgres("postgres://localhost/test"),
        );
        let mut metadata = DatabaseMetadata::new("test".into());
        metadata.table_summaries = ["users", "orders"]
            .into_iter()
            .map(|name| TableSummary::new("public".into(), name.into(), None))
            .collect();
        state.session.set_metadata(Some(Arc::new(metadata)));
        state
    }

    fn batch(effects: &[Effect]) -> (Arc<ConnectionEndpoint>, u64, Vec<(String, String)>) {
        match effects {
            [
                Effect::PrefetchTableColumnsAndFks {
                    endpoint,
                    run_id,
                    tables,
                },
            ] => (endpoint.clone(), *run_id, tables.clone()),
            _ => panic!("expected one batch effect"),
        }
    }

    #[test]
    fn fetches_all_missing_tables_in_one_batch_and_caches_results() {
        let mut state = state();
        let effects = start_completion_prefetch(
            &mut state,
            vec!["public.users".into(), "public.orders".into()],
        );
        let (endpoint, run, names) = batch(&effects);
        assert_eq!(
            names,
            [
                ("public".into(), "orders".into()),
                ("public".into(), "users".into())
            ]
        );
        let result = vec![
            table::minimal("public", "orders"),
            table::minimal("public", "users"),
        ];
        assert!(
            table_details_cached(&mut state, endpoint, run, Ok(result), Instant::now()).is_empty()
        );
        assert!(state.table_detail_cache.contains("public.users"));
        assert!(state.table_detail_cache.contains("public.orders"));
        assert!(state.table_prefetch.is_complete());
    }

    #[test]
    fn replacement_and_connection_switch_reject_old_results() {
        let mut state = state();
        let (endpoint, old, _) = batch(&start_completion_prefetch(
            &mut state,
            vec!["public.users".into()],
        ));
        let (_, current, names) = batch(&start_completion_prefetch(
            &mut state,
            vec!["public.orders".into()],
        ));
        assert_ne!(old, current);
        assert_eq!(names.len(), 2);
        table_details_cached(
            &mut state,
            endpoint.clone(),
            old,
            Ok(vec![table::minimal("public", "users")]),
            Instant::now(),
        );
        assert!(!state.table_detail_cache.contains("public.users"));
        state.session.activate_connection(
            &ConnectionId::new(),
            "other",
            &endpoint::postgres("postgres://localhost/other"),
        );
        table_details_cached(
            &mut state,
            endpoint,
            current,
            Ok(vec![table::minimal("public", "orders")]),
            Instant::now(),
        );
        assert!(!state.table_detail_cache.contains("public.orders"));
    }

    #[test]
    fn failure_is_not_retried_automatically_and_er_retry_fetches_it() {
        let mut state = state();
        let _ = state.er_preparation.start_waiting_run();
        let (endpoint, run, _) = batch(&start_er_prefetch_all(&mut state));
        let effects = table_details_cached(
            &mut state,
            endpoint,
            run,
            Err(DbOperationError::QueryFailed("denied".into())),
            Instant::now(),
        );
        assert_eq!(state.table_prefetch.failed_prefetch_count(), 2);
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, Effect::PrefetchTableColumnsAndFks { .. }))
        );
        state.table_prefetch.invalidate_prefetch();
        let _ = state.er_preparation.start_waiting_run();
        let (_, _, names) = batch(&start_er_prefetch_all(&mut state));
        assert_eq!(names.len(), 2);
        assert!(!state.table_prefetch.has_failures());
    }

    #[test]
    fn missing_table_is_recorded_as_failure_without_losing_successes() {
        let mut state = state();
        let (endpoint, run, _) = batch(&start_completion_prefetch(
            &mut state,
            vec!["public.users".into(), "public.orders".into()],
        ));
        table_details_cached(
            &mut state,
            endpoint,
            run,
            Ok(vec![table::minimal("public", "users")]),
            Instant::now(),
        );
        assert!(state.table_detail_cache.contains("public.users"));
        assert!(state.table_prefetch.failed_prefetch("public.orders"));
        assert!(start_completion_prefetch(&mut state, vec!["public.orders".into()]).is_empty());
    }
}
