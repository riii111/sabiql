use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::ConnectionTarget;
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::connection::helpers::cancel_connection_task_effects;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_connection_error(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::CloseConnectionError => {
            if state.session.pending_mysql_connection_probe().is_some() {
                state.connection_error.clear();
            } else {
                state.connection_error.reset_view();
                state.connection_error.clear_copied_feedback();
            }
            let cancel_effects = cancel_connection_task_effects(state);
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled_with(cancel_effects)
        }
        Action::ToggleConnectionErrorDetails => {
            state.connection_error.toggle_details();
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::ConnectionError,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.connection_error.scroll_up();
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::ConnectionError,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            // Approximation: uses raw line count, not wrapped line count.
            // Long lines that wrap in the UI may under-count; visible_height
            // is not subtracted. Acceptable for typical short psql errors.
            let max_scroll = state.connection_error.detail_line_count().saturating_sub(1);
            state.connection_error.scroll_down(max_scroll);
            DispatchResult::handled()
        }
        Action::CopyConnectionError => {
            if let Some(content) = state.connection_error.masked_details() {
                DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                    content: content.to_string(),
                    on_success: Some(Box::new(Action::ConnectionErrorCopied)),
                    on_failure: None,
                }])
            } else {
                DispatchResult::handled()
            }
        }
        Action::ConnectionErrorCopied => {
            state.connection_error.mark_copied_at(now);
            DispatchResult::handled()
        }
        Action::ReenterConnectionSetup => {
            if !state.connection_error.is_save_and_connect_failure()
                && (state.session.has_pending_connection_switch()
                    || !state.session.can_reenter_connection_setup())
            {
                return DispatchResult::handled();
            }
            state.connection_error.clear();
            state.session.cancel_connection_save();
            let cancel_effects = cancel_connection_task_effects(state);
            state.session.mark_disconnected();
            state.modal.replace_mode(InputMode::ConnectionSetup);
            DispatchResult::handled_with(cancel_effects)
        }
        Action::RetryConnection => {
            if state.connection_error.is_save_and_connect_failure() {
                return DispatchResult::handled();
            }
            if let Some(pending) = state.session.pending_mysql_connection_probe().cloned() {
                let target = ConnectionTarget {
                    id: pending.id,
                    dsn: pending.dsn,
                    name: pending.name,
                    database_type: DatabaseType::MySQL,
                    database: pending.database,
                };
                let run_id = state.session.begin_mysql_connection_probe(
                    &target.id,
                    &target.name,
                    &target.dsn,
                    target.database.as_deref(),
                );
                state.connection_error.clear();
                if state.session.dsn_matches(&target.dsn) {
                    state.session.mark_connecting();
                }
                state.modal.set_mode(InputMode::Normal);
                return DispatchResult::handled_with(vec![Effect::ProbeMySqlConnection {
                    target,
                    run_id,
                }]);
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
                return DispatchResult::handled();
            }
            if let Some(dsn) = state.session.dsn().map(String::from) {
                state.connection_error.clear();
                if state.session.active_database_type() == Some(DatabaseType::MySQL) {
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
                    let run_id = state.session.begin_mysql_connection_probe(
                        &target.id,
                        &target.name,
                        &target.dsn,
                        target.database.as_deref(),
                    );
                    state.session.mark_connecting();
                    state.modal.set_mode(InputMode::Normal);
                    return DispatchResult::handled_with(vec![Effect::ProbeMySqlConnection {
                        target,
                        run_id,
                    }]);
                }
                let run_id = state.session.begin_connecting(&dsn);
                state.session.disable_read_only();
                state.modal.set_mode(InputMode::Normal);
                DispatchResult::handled_with(vec![Effect::FetchMetadata { dsn, run_id }])
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
    use crate::domain::{ConnectionId, DatabaseType};
    use crate::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};
    use crate::model::connection::state::ConnectionState;
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
            state
                .connection_error
                .set_error(ConnectionErrorInfo::with_kind(
                    ConnectionErrorKind::Unknown,
                    "line1\nline2\nline3",
                ));

            let action = scroll_down_action();
            let now = Instant::now();

            reduce_connection_error(&mut state, &action, now);
            reduce_connection_error(&mut state, &action, now);
            assert_eq!(state.connection_error.scroll_offset(), 2);

            reduce_connection_error(&mut state, &action, now);
            assert_eq!(state.connection_error.scroll_offset(), 2);
        }
    }

    mod reenter_connection_setup {
        use super::*;

        #[test]
        fn blocked_for_service_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "service",
                DatabaseType::PostgreSQL,
                "service=mydb",
            );
            state.modal.set_mode(InputMode::ConnectionError);

            reduce_connection_error(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionError);
        }

        #[test]
        fn blocked_for_cli_ephemeral_connection() {
            use crate::cmd::cli_sqlite::connection_id_for_path;

            let mut state = AppState::new("test".to_string());
            state.session.activate_cli_ephemeral_connection(
                &connection_id_for_path("/tmp/app.db"),
                "app.db",
                "sqlite:///tmp/app.db",
            );
            state.modal.set_mode(InputMode::ConnectionError);

            reduce_connection_error(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionError);
        }

        #[test]
        fn allowed_for_profile_connection() {
            let mut state = AppState::new("test".to_string());
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/db");
            state.modal.set_mode(InputMode::ConnectionError);

            reduce_connection_error(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        }
    }

    #[test]
    fn retry_mysql_uses_probe_without_metadata() {
        let mut state = AppState::new("test".to_string());
        let id = ConnectionId::new();
        let dsn = "mysql://user@localhost:3306/app?ssl-mode=PREFERRED";
        state.session.activate_connection_with_target(
            &id,
            "mysql",
            DatabaseType::MySQL,
            dsn,
            Some("app"),
        );
        state.session.set_connection_state(ConnectionState::Failed);
        state.modal.set_mode(InputMode::ConnectionError);

        let effects = reduce_connection_error(&mut state, &Action::RetryConnection, Instant::now())
            .into_effects()
            .unwrap();

        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::ProbeMySqlConnection { target, .. }
                if target.id == id
                    && target.dsn == dsn
                    && target.database == Some("app".to_string())
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
        state.session.activate_connection_with_target(
            &ConnectionId::new(),
            "mysql",
            DatabaseType::MySQL,
            "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
            Some("app"),
        );
        state
            .connection_error
            .set_error(ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::Unknown,
                "connection failed",
            ));
        state.session.set_connection_state(ConnectionState::Failed);
        state.modal.set_mode(InputMode::ConnectionError);

        let effects = reduce_connection_error(&mut state, &Action::RetryConnection, Instant::now())
            .into_effects()
            .expect("retry action handled");

        assert!(effects.is_empty());
        assert_eq!(state.input_mode(), InputMode::ConnectionError);
    }

    #[test]
    fn close_after_mysql_switch_failure_clears_failed_probe_context() {
        let mut state = AppState::new("test".to_string());
        let current_id = ConnectionId::from_string("mysql-a");
        let target_id = ConnectionId::from_string("mysql-b");
        state.session.activate_connection_with_target(
            &current_id,
            "mysql-a",
            DatabaseType::MySQL,
            "mysql://user@localhost:3306/a?ssl-mode=PREFERRED",
            Some("a"),
        );
        state
            .session
            .set_connection_state(ConnectionState::Connected);

        let _ = state.session.begin_mysql_connection_probe(
            &target_id,
            "mysql-b",
            "mysql://user@localhost:3306/b?ssl-mode=PREFERRED",
            Some("b"),
        );
        state
            .connection_error
            .set_error(ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::ConnectionRefused,
                "connection refused",
            ));
        state.modal.set_mode(InputMode::ConnectionError);

        reduce_connection_error(&mut state, &Action::CloseConnectionError, Instant::now());

        assert!(state.session.pending_mysql_connection_probe().is_none());
        assert!(state.connection_error.error_info().is_none());
        assert_eq!(state.input_mode(), InputMode::Normal);
        assert!(state.session.connection_state().is_connected());
        assert_eq!(
            state.session.dsn(),
            Some("mysql://user@localhost:3306/a?ssl-mode=PREFERRED")
        );
    }

    #[test]
    fn close_after_same_mysql_retry_clears_failed_probe_context() {
        let mut state = AppState::new("test".to_string());
        let id = ConnectionId::from_string("mysql-a");
        let dsn = "mysql://user@localhost:3306/a?ssl-mode=PREFERRED";
        state.session.activate_connection_with_target(
            &id,
            "mysql-a",
            DatabaseType::MySQL,
            dsn,
            Some("a"),
        );
        let _ = state
            .session
            .begin_mysql_connection_probe(&id, "mysql-a", dsn, Some("a"));
        state
            .connection_error
            .set_error(ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::ConnectionRefused,
                "connection refused",
            ));
        state.modal.set_mode(InputMode::ConnectionError);

        assert!(!state.session.has_pending_connection_switch());
        reduce_connection_error(&mut state, &Action::CloseConnectionError, Instant::now());

        assert!(state.session.pending_mysql_connection_probe().is_none());
        assert!(state.connection_error.error_info().is_none());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }

    #[test]
    fn reenter_after_mysql_switch_failure_preserves_active_connection() {
        let mut state = AppState::new("test".to_string());
        let current_id = ConnectionId::from_string("mysql-a");
        let target_id = ConnectionId::from_string("mysql-b");
        state.session.activate_connection_with_target(
            &current_id,
            "mysql-a",
            DatabaseType::MySQL,
            "mysql://user@localhost:3306/a?ssl-mode=PREFERRED",
            Some("a"),
        );
        state
            .session
            .set_connection_state(ConnectionState::Connected);

        let _ = state.session.begin_mysql_connection_probe(
            &target_id,
            "mysql-b",
            "mysql://user@localhost:3306/b?ssl-mode=PREFERRED",
            Some("b"),
        );
        state
            .connection_error
            .set_error(ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::ConnectionRefused,
                "connection refused",
            ));
        state.modal.set_mode(InputMode::ConnectionError);

        reduce_connection_error(&mut state, &Action::ReenterConnectionSetup, Instant::now());

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
            state.session.dsn(),
            Some("mysql://user@localhost:3306/a?ssl-mode=PREFERRED")
        );
    }
}
