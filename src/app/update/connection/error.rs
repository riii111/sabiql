use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::ConnectionTarget;
use crate::update::action::CopyTarget;
use crate::update::action::{Action, ScrollAmount, ScrollDirection};
use crate::update::connection::helpers::cancel_connection_task_effects;

pub(in crate::update) fn close_connection_error(state: &mut AppState) -> Vec<Effect> {
    if state.session.pending_mysql_connection_probe().is_some() {
        state.connection_error.clear();
    } else {
        state.connection_error.reset_view();
        state.connection_error.clear_copied_feedback();
    }
    let cancel_effects = cancel_connection_task_effects(state);
    state.modal.set_mode(InputMode::Normal);
    cancel_effects
}

pub(in crate::update) fn scroll_connection_error(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match (direction, amount) {
        (ScrollDirection::Up, ScrollAmount::Line) => {
            state.connection_error.scroll_up();
        }
        (ScrollDirection::Down, ScrollAmount::Line) => {
            // Approximation: uses raw line count, not wrapped line count.
            // Long lines that wrap in the UI may under-count; visible_height
            // is not subtracted. Acceptable for typical short psql errors.
            let max_scroll = state.connection_error.detail_line_count().saturating_sub(1);
            state.connection_error.scroll_down(max_scroll);
        }
        _ => debug_assert!(
            false,
            "unsupported ConnectionError scroll: {direction:?} {amount:?}"
        ),
    }
}

pub(in crate::update) fn copy_connection_error(state: &AppState) -> Vec<Effect> {
    if let Some(content) = state.connection_error.masked_details() {
        vec![Effect::CopyToClipboard {
            content: content.to_string(),
            on_success: Box::new(Action::CopySucceeded(CopyTarget::ConnectionError)),
            on_failure: None,
        }]
    } else {
        vec![]
    }
}

pub(in crate::update) fn reenter_connection_setup(state: &mut AppState) -> Vec<Effect> {
    if !state.connection_error.is_save_and_connect_failure()
        && (state.session.has_pending_connection_switch()
            || !state.session.can_reenter_connection_setup())
    {
        return vec![];
    }
    state.connection_error.clear();
    state.session.cancel_connection_save();
    let cancel_effects = cancel_connection_task_effects(state);
    state.session.mark_disconnected();
    state.modal.replace_mode(InputMode::ConnectionSetup);
    cancel_effects
}

pub(in crate::update) fn retry_connection(state: &mut AppState) -> Vec<Effect> {
    if state.connection_error.is_save_and_connect_failure() {
        return vec![];
    }
    if let Some(pending) = state.session.pending_mysql_connection_probe().cloned() {
        let target = ConnectionTarget {
            id: pending.id,
            name: pending.name,
            endpoint: pending.endpoint,
        };
        let run_id =
            state
                .session
                .begin_mysql_connection_probe(&target.id, &target.name, &target.endpoint);
        state.connection_error.clear();
        if state.session.endpoint_matches(&target.endpoint) {
            state.session.mark_connecting();
        }
        state.modal.set_mode(InputMode::Normal);
        return vec![Effect::ProbeMySqlConnection { target, run_id }];
    }
    if state
        .session
        .active_database_type()
        .is_some_and(|database_type| database_type == DatabaseType::MySQL)
        && state
            .connection_error
            .error_info()
            .is_some_and(|info| !info.is_retryable())
    {
        return vec![];
    }
    if let Some(endpoint) = state.session.endpoint().cloned() {
        state.connection_error.clear();
        if state.session.active_database_type() == Some(DatabaseType::MySQL) {
            let target = ConnectionTarget {
                id: state
                    .session
                    .active_connection_id()
                    .cloned()
                    .expect("active MySQL connection"),
                name: state
                    .session
                    .active_connection_name()
                    .unwrap_or_default()
                    .to_string(),
                endpoint,
            };
            let run_id = state.session.begin_mysql_connection_probe(
                &target.id,
                &target.name,
                &target.endpoint,
            );
            state.session.mark_connecting();
            state.modal.set_mode(InputMode::Normal);
            return vec![Effect::ProbeMySqlConnection { target, run_id }];
        }
        let run_id = state.session.begin_connecting();
        state.session.disable_read_only();
        state.modal.set_mode(InputMode::Normal);
        vec![Effect::FetchMetadata { endpoint, run_id }]
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::domain::ConnectionId;
    use crate::model::connection::error::test_support;
    use crate::model::connection::state::ConnectionState;
    use crate::services::AppServices;
    use crate::test_support::endpoint;
    use crate::update::action::ScrollTarget;
    use crate::update::reducer::route;
    use crate::update::test_fixtures;

    mod scroll_down {
        use super::*;

        fn scroll_down_action() -> Action {
            Action::Scroll {
                target: ScrollTarget::ConnectionError,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        }

        #[test]
        fn stops_at_detail_line_count() {
            let mut state = AppState::new("test".to_string());
            state.connection_error.set_error(test_support::from_parts(
                "Connection failed",
                "See details for more information",
                false,
                "line1\nline2\nline3",
            ));

            let action = scroll_down_action();

            route(
                &mut state,
                action.clone(),
                Instant::now(),
                &AppServices::stub(),
            );
            route(
                &mut state,
                action.clone(),
                Instant::now(),
                &AppServices::stub(),
            );
            assert_eq!(state.connection_error.scroll_offset(), 2);

            route(&mut state, action, Instant::now(), &AppServices::stub());
            assert_eq!(state.connection_error.scroll_offset(), 2);
        }
    }

    mod reenter_connection_setup {
        use super::*;

        #[test]
        fn blocked_for_service_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection(
                &ConnectionId::new(),
                "service",
                &endpoint::service("mydb"),
            );
            state.modal.set_mode(InputMode::ConnectionError);

            route(
                &mut state,
                Action::ReenterConnectionSetup,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::ConnectionError);
        }

        #[test]
        fn blocked_for_cli_ephemeral_connection() {
            use crate::domain::{ConnectionId, SqliteConnectionConfig};

            let mut state = AppState::new("test".to_string());
            state.session.activate_cli_ephemeral_connection(
                &ConnectionId::from_string("cli-sqlite-test"),
                "app.db",
                SqliteConnectionConfig::new("/tmp/app.db").unwrap(),
            );
            state.modal.set_mode(InputMode::ConnectionError);

            route(
                &mut state,
                Action::ReenterConnectionSetup,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::ConnectionError);
        }

        #[test]
        fn allowed_for_profile_connection() {
            let mut state = AppState::new("test".to_string());
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/db");
            state.modal.set_mode(InputMode::ConnectionError);

            route(
                &mut state,
                Action::ReenterConnectionSetup,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        }
    }

    #[test]
    fn retry_mysql_uses_probe_without_metadata() {
        let mut state = AppState::new("test".to_string());
        let id = ConnectionId::new();
        let dsn = "mysql://user@localhost:3306/app?ssl-mode=PREFERRED";
        state
            .session
            .activate_connection(&id, "mysql", &endpoint::mysql(dsn, Some("app")));
        state.session.set_connection_state(ConnectionState::Failed);
        state.modal.set_mode(InputMode::ConnectionError);

        let effects = route(
            &mut state,
            Action::RetryConnection,
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::ProbeMySqlConnection { target, .. }
                if target.id == id
                    && target.endpoint == endpoint::mysql(dsn, Some("app"))
                    && target.endpoint.selected_database() == Some("app")
        )));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
        );
        assert!(state.session.connection_state().is_connecting());
    }

    #[test]
    fn retry_mysql_ignores_non_retryable_error() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection(
            &ConnectionId::new(),
            "mysql",
            &endpoint::mysql(
                "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
                Some("app"),
            ),
        );
        state.connection_error.set_error(test_support::from_parts(
            "Connection failed",
            "See details for more information",
            false,
            "connection failed",
        ));
        state.session.set_connection_state(ConnectionState::Failed);
        state.modal.set_mode(InputMode::ConnectionError);

        let effects = route(
            &mut state,
            Action::RetryConnection,
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(effects.is_empty());
        assert_eq!(state.input_mode(), InputMode::ConnectionError);
    }

    #[test]
    fn close_after_mysql_switch_failure_clears_failed_probe_context() {
        let mut state = AppState::new("test".to_string());
        let current_id = ConnectionId::from_string("mysql-a");
        let target_id = ConnectionId::from_string("mysql-b");
        state.session.activate_connection(
            &current_id,
            "mysql-a",
            &endpoint::mysql(
                "mysql://user@localhost:3306/a?ssl-mode=PREFERRED",
                Some("a"),
            ),
        );
        state
            .session
            .set_connection_state(ConnectionState::Connected);

        let _ = state.session.begin_mysql_connection_probe(
            &target_id,
            "mysql-b",
            &endpoint::mysql(
                "mysql://user@localhost:3306/b?ssl-mode=PREFERRED",
                Some("b"),
            ),
        );
        state.connection_error.set_error(test_support::from_parts(
            "Connection refused",
            "Check the host, port, and server availability",
            true,
            "connection refused",
        ));
        state.modal.set_mode(InputMode::ConnectionError);

        route(
            &mut state,
            Action::CloseConnectionError,
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(state.session.pending_mysql_connection_probe().is_none());
        assert!(state.connection_error.error_info().is_none());
        assert_eq!(state.input_mode(), InputMode::Normal);
        assert!(state.session.connection_state().is_connected());
        assert_eq!(
            state.session.endpoint(),
            Some(&endpoint::mysql(
                "mysql://user@localhost:3306/a?ssl-mode=PREFERRED",
                Some("a")
            ))
        );
    }

    #[test]
    fn close_after_same_mysql_retry_clears_failed_probe_context() {
        let mut state = AppState::new("test".to_string());
        let id = ConnectionId::from_string("mysql-a");
        let dsn = "mysql://user@localhost:3306/a?ssl-mode=PREFERRED";
        state
            .session
            .activate_connection(&id, "mysql-a", &endpoint::mysql(dsn, Some("a")));
        let _ = state.session.begin_mysql_connection_probe(
            &id,
            "mysql-a",
            &endpoint::mysql(dsn, Some("a")),
        );
        state.connection_error.set_error(test_support::from_parts(
            "Connection refused",
            "Check the host, port, and server availability",
            true,
            "connection refused",
        ));
        state.modal.set_mode(InputMode::ConnectionError);

        assert!(!state.session.has_pending_connection_switch());
        route(
            &mut state,
            Action::CloseConnectionError,
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(state.session.pending_mysql_connection_probe().is_none());
        assert!(state.connection_error.error_info().is_none());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn reenter_after_mysql_switch_failure_preserves_active_connection() {
        let mut state = AppState::new("test".to_string());
        let current_id = ConnectionId::from_string("mysql-a");
        let target_id = ConnectionId::from_string("mysql-b");
        state.session.activate_connection(
            &current_id,
            "mysql-a",
            &endpoint::mysql(
                "mysql://user@localhost:3306/a?ssl-mode=PREFERRED",
                Some("a"),
            ),
        );
        state
            .session
            .set_connection_state(ConnectionState::Connected);

        let _ = state.session.begin_mysql_connection_probe(
            &target_id,
            "mysql-b",
            &endpoint::mysql(
                "mysql://user@localhost:3306/b?ssl-mode=PREFERRED",
                Some("b"),
            ),
        );
        state.connection_error.set_error(test_support::from_parts(
            "Connection refused",
            "Check the host, port, and server availability",
            true,
            "connection refused",
        ));
        state.modal.set_mode(InputMode::ConnectionError);

        route(
            &mut state,
            Action::ReenterConnectionSetup,
            Instant::now(),
            &AppServices::stub(),
        );

        assert_eq!(state.input_mode(), InputMode::ConnectionError);
        assert_eq!(
            state
                .session
                .pending_mysql_connection_probe()
                .map(|pending| &pending.id),
            Some(&target_id)
        );
        assert!(state.session.connection_state().is_connected());
        assert_eq!(
            state.session.endpoint(),
            Some(&endpoint::mysql(
                "mysql://user@localhost:3306/a?ssl-mode=PREFERRED",
                Some("a")
            ))
        );
    }
}
