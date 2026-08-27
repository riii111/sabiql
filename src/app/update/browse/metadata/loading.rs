use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::connection::error::ConnectionErrorInfo;
use crate::model::er_state::ErStatus;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ModalKind};
use crate::update::browse::query::preview_effect_for_current_table;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::{metadata_reload_effects, reject_pending_mysql_connection_probe};
use crate::update::query_context::termination_effects;

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
            if !state.session.dsn_matches(dsn) || !state.session.is_current_metadata_run(*run_id) {
                return DispatchResult::handled();
            }

            let has_tables = !metadata.table_summaries.is_empty();
            state.session.mark_connected(Arc::clone(metadata));
            let effective_user_run_id = state.session.begin_effective_user_fetch();

            let mut effects = vec![Effect::FetchEffectiveUser {
                dsn: dsn.clone(),
                run_id: effective_user_run_id,
            }];

            if state.query.pagination.table().is_empty() {
                state
                    .ui
                    .set_explorer_selection(if has_tables { Some(0) } else { None });
            } else {
                let prev_schema = state.query.pagination.schema();
                let prev_table = state.query.pagination.table();
                let found_index = metadata
                    .table_summaries
                    .iter()
                    .position(|t| t.schema == prev_schema && t.name == prev_table);
                if let Some(idx) = found_index {
                    state.ui.set_explorer_selection(Some(idx));
                    // Refresh preview and detail: DDL or reload may have changed
                    // data/schema even though the table still exists.
                    let page = state.query.pagination.current_page();
                    let generation = state.session.selection_generation();
                    effects.extend(preview_effect_for_current_table(
                        state, now, page, generation,
                    ));
                    let detail_run_id = state.session.begin_table_detail_run();
                    effects.push(Effect::FetchTableDetail {
                        dsn: dsn.clone(),
                        schema: state.query.pagination.schema().to_string(),
                        table: state.query.pagination.table().to_string(),
                        generation,
                        run_id: detail_run_id,
                    });
                } else {
                    // The previously selected table was removed (e.g. via DROP TABLE).
                    // Clear all selection state to avoid stale references.
                    state
                        .ui
                        .set_explorer_selection(if has_tables { Some(0) } else { None });
                    state.session.clear_table_selection(&mut state.query);
                    state.query.clear_current_result();
                    effects.extend(termination_effects(&state.query, vec![]));
                }
            }

            state.connection_error.clear();

            if state.session.is_reloading() {
                state.messages.set_success_at("Reloaded!".to_string(), now);
                state.session.finish_reload();
            }

            if state.ui.take_pending_er_picker() && state.modal.active_mode() == InputMode::Normal {
                effects.push(Effect::DispatchActions(vec![Action::OpenModal(
                    ModalKind::ErTablePicker,
                )]));
            }

            DispatchResult::handled_with(effects)
        }
        Action::EffectiveUserLoaded {
            dsn,
            run_id,
            effective_user,
        } => {
            if !state.session.dsn_matches(dsn)
                || !state.session.is_current_effective_user_run(*run_id)
            {
                return DispatchResult::handled();
            }

            state
                .session
                .mark_effective_user_loaded(effective_user.clone());
            DispatchResult::handled()
        }
        Action::MetadataFailed { dsn, run_id, error } => {
            if !state.session.dsn_matches(dsn) || !state.session.is_current_metadata_run(*run_id) {
                return DispatchResult::handled();
            }

            let error_info = ConnectionErrorInfo::from_db_operation_error(error);
            state.connection_error.set_error(error_info);
            let was_connected = state.session.connection_state().is_connected();
            state.session.mark_connection_failed();
            if !was_connected {
                state.session.set_metadata(None);
                state.session.clear_table_selection(&mut state.query);
                state.query.clear_current_result();
                state.ui.set_explorer_selection(None);
                state.result_interaction.reset_view();
                state.modal.replace_mode(InputMode::ConnectionError);
            }
            if state.er_preparation.status() == ErStatus::Waiting {
                state.er_preparation.mark_idle();
            }
            DispatchResult::handled_with(if was_connected {
                if state
                    .session
                    .mark_metadata_detail_failed(error.user_message())
                {
                    super::table_detail::reveal_pending_preview(
                        state,
                        state.session.selection_generation(),
                    )
                } else {
                    vec![]
                }
            } else {
                termination_effects(&state.query, vec![])
            })
        }
        Action::ReloadMetadata => {
            if reject_pending_mysql_connection_probe(state) {
                return DispatchResult::handled();
            }
            if let Some(dsn) = state.session.dsn().map(String::from) {
                state.sql_modal.reset_prefetch();
                state.er_preparation.reset();
                state.ui.reset_er_picker_request();
                state.messages.clear();

                DispatchResult::handled_with(metadata_reload_effects(state, &dsn))
            } else {
                DispatchResult::handled()
            }
        }
        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConnectionId, DatabaseMetadata, DatabaseType, QueryResult, QuerySource};
    use crate::model::browse::session::TableDetailState;
    use crate::ports::outbound::DbOperationError;
    use crate::services::AppServices;
    use crate::test_support::table;
    use crate::update::browse::query::dispatch_query;

    fn connected_state(database_type: DatabaseType) -> AppState {
        let (name, dsn) = match database_type {
            DatabaseType::PostgreSQL => ("postgres", "postgres://localhost/test"),
            DatabaseType::SQLite => ("sqlite", "sqlite:///tmp/test.db"),
            DatabaseType::MySQL => ("mysql", "mysql://localhost/test"),
        };
        let mut state = AppState::new("test".to_string());
        state
            .session
            .activate_connection_with_dsn(&ConnectionId::new(), name, database_type, dsn);
        state
            .session
            .mark_connected(Arc::new(DatabaseMetadata::new("test".to_string())));
        state
    }

    fn loading_detail_and_metadata_run(state: &mut AppState) -> (u64, u64) {
        let generation = state
            .session
            .select_table("public", "users", &mut state.query);
        let run_id = state.session.begin_metadata_refresh();
        (generation, run_id)
    }

    fn metadata_failed(dsn: &str, run_id: u64) -> Action {
        Action::MetadataFailed {
            dsn: dsn.to_string(),
            run_id,
            error: DbOperationError::PermissionDenied("metadata refresh failed".to_string()),
        }
    }

    #[test]
    fn metadata_failure_terminates_initial_loading_detail() {
        let mut state = connected_state(DatabaseType::PostgreSQL);
        let (generation, run_id) = loading_detail_and_metadata_run(&mut state);
        let dsn = state.session.dsn().expect("connected DSN").to_string();

        let effects = reduce_loading(&mut state, &metadata_failed(&dsn, run_id), Instant::now())
            .into_effects()
            .expect("metadata failure should be handled");

        assert!(matches!(
            state.session.table_detail_state(),
            TableDetailState::Error(_)
        ));
        assert!(state.session.is_table_detail_terminal(generation));
        assert!(effects.is_empty());
    }

    #[rstest::rstest]
    #[case(DatabaseType::PostgreSQL)]
    #[case(DatabaseType::SQLite)]
    #[case(DatabaseType::MySQL)]
    fn metadata_failure_terminates_existing_detail_for_each_database(
        #[case] database_type: DatabaseType,
    ) {
        let mut state = connected_state(database_type);
        let generation = state
            .session
            .select_table("public", "users", &mut state.query);
        assert!(
            state
                .session
                .set_table_detail(table::minimal("public", "users"), generation,)
        );
        state.session.set_table_detail_raw(None);
        assert!(matches!(
            state.session.table_detail_state(),
            TableDetailState::Loading
        ));
        let run_id = state.session.begin_metadata_refresh();
        let dsn = state.session.dsn().expect("connected DSN").to_string();

        let effects = reduce_loading(&mut state, &metadata_failed(&dsn, run_id), Instant::now())
            .into_effects()
            .expect("metadata failure should be handled");

        assert!(matches!(
            state.session.table_detail_state(),
            TableDetailState::Error(_)
        ));
        assert!(state.session.is_table_detail_terminal(generation));
        assert!(effects.is_empty());
    }

    #[test]
    fn metadata_failure_reveals_pending_preview_after_terminalizing_detail() {
        let mut state = connected_state(DatabaseType::SQLite);
        let (generation, run_id) = loading_detail_and_metadata_run(&mut state);
        let dsn = state.session.dsn().expect("connected DSN").to_string();
        state.query.defer_preview(
            Arc::new(QueryResult::error(
                "SELECT * FROM public.users".to_string(),
                "preview failed".to_string(),
                0,
                QuerySource::Preview,
            )),
            generation,
            None,
            false,
        );

        let effects = reduce_loading(&mut state, &metadata_failed(&dsn, run_id), Instant::now())
            .into_effects()
            .expect("metadata failure should be handled");

        assert!(matches!(
            effects.as_slice(),
            [Effect::DispatchActions(actions)]
                if matches!(
                    actions.as_slice(),
                    [Action::RevealPendingPreview { generation: action_generation }]
                        if *action_generation == generation
                )
        ));
        assert!(state.session.is_table_detail_terminal(generation));
        dispatch_query(
            &mut state,
            &Action::RevealPendingPreview { generation },
            Instant::now(),
            &AppServices::stub(),
        );
        assert!(state.query.current_result().is_some());
        assert!(!state.query.has_pending_preview(generation));
    }

    #[test]
    fn reload_failure_preserves_existing_detail_snapshot() {
        let mut state = connected_state(DatabaseType::PostgreSQL);
        let generation = state
            .session
            .select_table("public", "users", &mut state.query);
        let detail = table::minimal("public", "users");
        assert!(state.session.set_table_detail(detail, generation));

        reduce_loading(&mut state, &Action::ReloadMetadata, Instant::now())
            .into_effects()
            .expect("metadata reload should be handled");
        let run_id = state.session.metadata_generation();
        let dsn = state.session.dsn().expect("connected DSN").to_string();

        let effects = reduce_loading(&mut state, &metadata_failed(&dsn, run_id), Instant::now())
            .into_effects()
            .expect("metadata failure should be handled");

        assert!(effects.is_empty());
        assert!(matches!(
            state.session.table_detail_state(),
            TableDetailState::Loaded(_)
        ));
        assert!(matches!(
            state.session.table_detail(),
            Some(detail) if detail.schema == "public" && detail.name == "users"
        ));
        assert!(state.session.is_table_detail_terminal(generation));
    }

    #[test]
    fn stale_metadata_failure_does_not_terminate_new_loading_detail() {
        let mut state = connected_state(DatabaseType::PostgreSQL);
        let _ = state
            .session
            .select_table("public", "users", &mut state.query);
        let stale_run_id = state.session.begin_metadata_refresh();
        let generation = state
            .session
            .select_table("public", "orders", &mut state.query);
        let dsn = state.session.dsn().expect("connected DSN").to_string();

        let effects = reduce_loading(
            &mut state,
            &metadata_failed(&dsn, stale_run_id),
            Instant::now(),
        )
        .into_effects()
        .expect("stale metadata failure should be handled");

        assert!(effects.is_empty());
        assert!(matches!(
            state.session.table_detail_state(),
            TableDetailState::Loading
        ));
        assert!(!state.session.is_table_detail_terminal(generation));
    }

    #[test]
    fn stale_metadata_failure_preserves_new_loaded_detail() {
        let mut state = connected_state(DatabaseType::SQLite);
        let _ = state
            .session
            .select_table("main", "users", &mut state.query);
        let stale_run_id = state.session.begin_metadata_refresh();
        let generation = state
            .session
            .select_table("main", "orders", &mut state.query);
        let detail = table::minimal("main", "orders");
        assert!(state.session.set_table_detail(detail, generation));
        let dsn = state.session.dsn().expect("connected DSN").to_string();

        let effects = reduce_loading(
            &mut state,
            &metadata_failed(&dsn, stale_run_id),
            Instant::now(),
        )
        .into_effects()
        .expect("stale metadata failure should be handled");

        assert!(effects.is_empty());
        assert!(matches!(
            state.session.table_detail(),
            Some(detail) if detail.schema == "main" && detail.name == "orders"
        ));
        assert!(state.session.is_table_detail_terminal(generation));
    }

    #[test]
    fn stale_metadata_failure_does_not_terminate_current_detail() {
        let mut state = connected_state(DatabaseType::MySQL);
        let (generation, stale_run_id) = loading_detail_and_metadata_run(&mut state);
        let current_run_id = state.session.begin_metadata_refresh();
        let dsn = state.session.dsn().expect("connected DSN").to_string();

        let effects = reduce_loading(
            &mut state,
            &metadata_failed(&dsn, stale_run_id),
            Instant::now(),
        )
        .into_effects()
        .expect("stale metadata failure should be handled");

        assert!(effects.is_empty());
        assert!(matches!(
            state.session.table_detail_state(),
            TableDetailState::Loading
        ));
        assert!(!state.session.is_table_detail_terminal(generation));
        assert!(state.session.is_current_metadata_run(current_run_id));
    }
}
