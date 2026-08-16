use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::connection::error::ConnectionErrorInfo;
use crate::model::shared::input_mode::InputMode;
use crate::services::AppServices;
use crate::update::action::{Action, ConnectionTarget};
use crate::update::query_context::termination_effects;

use crate::update::dispatch_result::DispatchResult;

use super::helpers::{
    mysql_connection_completion_effects, reset_for_new_connection, restore_cache,
    save_current_cache,
};

pub fn reduce_connection_lifecycle(
    state: &mut AppState,
    action: &Action,
    now: std::time::Instant,
    _services: &AppServices,
) -> DispatchResult {
    match action {
        Action::TryConnect => {
            if state.session.connection_state().is_not_connected()
                && state.modal.active_mode() == InputMode::Normal
            {
                if let Some(dsn) = state.session.dsn().map(str::to_string) {
                    if state.session.active_database_type() == Some(DatabaseType::MySQL) {
                        if state.session.active_database().is_none() {
                            state.messages.set_error_at(
                                "MySQL connection field `database` is required".to_string(),
                                now,
                            );
                            return DispatchResult::handled();
                        }
                        let target = ConnectionTarget {
                            id: state
                                .session
                                .active_connection_id()
                                .cloned()
                                .expect("active MySQL connection"),
                            dsn,
                            name: state
                                .session
                                .active_connection_name()
                                .unwrap_or_default()
                                .to_string(),
                            database_type: DatabaseType::MySQL,
                            database: state.session.active_database().map(str::to_string),
                        };
                        let run_id = state.session.begin_connection_probe(
                            &target.id,
                            &target.name,
                            target.database_type,
                            &target.dsn,
                            target.database.as_deref(),
                        );
                        state.session.mark_connecting();
                        return DispatchResult::handled_with(vec![Effect::ProbeConnection {
                            target,
                            run_id,
                        }]);
                    }
                    let run_id = state.session.begin_connecting(&dsn);
                    DispatchResult::handled_with(vec![Effect::FetchMetadata { dsn, run_id }])
                } else {
                    DispatchResult::handled()
                }
            } else {
                DispatchResult::handled()
            }
        }

        Action::SwitchConnection(target) => {
            state.connection_error.clear();
            let ConnectionTarget {
                id,
                dsn,
                name,
                database_type,
                database,
            } = target;

            if *database_type == DatabaseType::MySQL && database.is_none() {
                state.messages.set_error_at(
                    "MySQL connection field `database` is required".to_string(),
                    now,
                );
                return DispatchResult::handled();
            }

            if state.session.active_database_type() != Some(DatabaseType::MySQL)
                && let Some(current_id) = state.session.active_connection_id().cloned()
            {
                let cache = save_current_cache(state);
                state.connection_caches.save(&current_id, cache);
            }

            if *database_type == DatabaseType::MySQL {
                let run_id = state.session.begin_connection_probe(
                    id,
                    name,
                    *database_type,
                    dsn,
                    database.as_deref(),
                );
                state.query.reset_for_context_change();
                return DispatchResult::handled_with(termination_effects(
                    &state.query,
                    vec![Effect::ProbeConnection {
                        target: target.clone(),
                        run_id,
                    }],
                ));
            }

            state.session.clear_connection_probe();

            if let Some(cached) = state.connection_caches.get(id).cloned() {
                restore_cache(state, &cached, target);
                let mut effects = vec![Effect::ClearCompletionEngineCache];
                if state.session.effective_user().is_none() {
                    let run_id = state.session.begin_effective_user_fetch();
                    effects.push(Effect::FetchEffectiveUser {
                        dsn: dsn.clone(),
                        run_id,
                    });
                }
                DispatchResult::handled_with(termination_effects(&state.query, effects))
            } else {
                // No cache: reset and fetch metadata
                reset_for_new_connection(state, id, dsn, name, *database_type, database.as_deref());
                let run_id = state.session.begin_connecting(dsn);
                DispatchResult::handled_with(termination_effects(
                    &state.query,
                    vec![
                        Effect::ClearCompletionEngineCache,
                        Effect::FetchMetadata {
                            dsn: dsn.clone(),
                            run_id,
                        },
                    ],
                ))
            }
        }

        Action::ConnectionProbeCompleted { target, run_id } => {
            let ConnectionTarget {
                id,
                dsn,
                name,
                database_type,
                database,
            } = target;
            if *database_type != DatabaseType::MySQL
                || !state.session.is_current_connection_probe(
                    id,
                    name,
                    *database_type,
                    dsn,
                    database.as_deref(),
                    *run_id,
                )
            {
                return DispatchResult::handled();
            }
            reset_for_new_connection(state, id, dsn, name, *database_type, database.as_deref());
            DispatchResult::handled_with(mysql_connection_completion_effects(state, dsn))
        }

        Action::ConnectionProbeFailed {
            target,
            run_id,
            error,
        } => {
            if target.database_type != DatabaseType::MySQL
                || !state.session.is_current_connection_probe(
                    &target.id,
                    &target.name,
                    target.database_type,
                    &target.dsn,
                    target.database.as_deref(),
                    *run_id,
                )
            {
                return DispatchResult::handled();
            }
            let message = error.user_message();
            let table_detail_retry = if state.session.dsn_matches(&target.dsn) {
                None
            } else {
                state.session.retry_table_detail_after_probe_failure().map(
                    |(dsn, generation, run_id)| Effect::FetchTableDetail {
                        dsn,
                        schema: state.query.pagination.schema().to_string(),
                        table: state.query.pagination.table().to_string(),
                        generation,
                        run_id,
                    },
                )
            };
            if state.session.dsn_matches(&target.dsn) {
                state
                    .session
                    .mark_table_detail_probe_failed(&target.dsn, message.clone());
                state.session.mark_connection_failed(message);
            }
            state.connection_error.set_error(
                ConnectionErrorInfo::from_db_operation_error_with_dsn(error, &target.dsn),
            );
            state.modal.replace_mode(InputMode::ConnectionError);
            DispatchResult::handled_with(table_detail_retry.into_iter().collect())
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
