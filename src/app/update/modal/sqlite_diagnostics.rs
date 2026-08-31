use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_sqlite_diagnostics(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::SqliteDiagnostics) => {
            let Some(dsn) = state.session.dsn().map(String::from) else {
                return DispatchResult::handled();
            };
            let run_id = state.sqlite_diagnostics.begin_core_fetch();
            state.modal.set_mode(InputMode::SqliteDiagnostics);
            DispatchResult::handled_with(vec![Effect::FetchSqliteDiagnosticsCore { dsn, run_id }])
        }
        Action::RunSqliteDiagnosticsQuickCheck => {
            let Some(dsn) = state.session.dsn().map(String::from) else {
                return DispatchResult::handled();
            };
            let Some(run_id) = state.sqlite_diagnostics.begin_quick_check() else {
                return DispatchResult::handled();
            };
            DispatchResult::handled_with(vec![Effect::FetchSqliteDiagnosticsQuickCheck {
                dsn,
                run_id,
            }])
        }
        Action::CloseModal(ModalKind::SqliteDiagnostics) => {
            state.sqlite_diagnostics.clear();
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled_with(vec![Effect::CancelSqliteDiagnostics])
        }
        Action::SqliteDiagnosticsCoreLoaded { run_id, snapshot } => {
            if !state.sqlite_diagnostics.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            state
                .sqlite_diagnostics
                .set_core_loaded(*run_id, snapshot.as_ref().clone());
            DispatchResult::handled()
        }
        Action::SqliteDiagnosticsQuickCheckLoaded {
            run_id,
            quick_check,
        } => {
            if !state.sqlite_diagnostics.is_current_run(*run_id) {
                return DispatchResult::handled();
            }
            state
                .sqlite_diagnostics
                .set_quick_check_loaded(*run_id, quick_check.clone());
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::SqliteDiagnostics,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.sqlite_diagnostics.scroll_up();
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::SqliteDiagnostics,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            state.sqlite_diagnostics.scroll_down();
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::DatabaseType;
    use crate::domain::{ConnectionId, DiagnosticField, SqliteDiagnosticsSnapshot};
    use crate::services::AppServices;
    use crate::update::reducer::reduce;
    use crate::update::test_fixtures;

    fn reduce_at_boundary(state: &mut AppState, action: Action) -> Vec<Effect> {
        reduce(state, action, Instant::now(), &AppServices::stub())
    }

    #[test]
    fn open_starts_split_fetch_for_sqlite_connection() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");

        let effects =
            reduce_at_boundary(&mut state, Action::OpenModal(ModalKind::SqliteDiagnostics));

        assert_eq!(state.input_mode(), InputMode::SqliteDiagnostics);
        assert_eq!(effects.len(), 1);
        assert!(matches!(
            effects[0],
            Effect::FetchSqliteDiagnosticsCore { .. }
        ));
    }

    #[test]
    fn run_quick_check_starts_read_only_effect_for_loaded_snapshot() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        let run_id = state.sqlite_diagnostics.begin_core_fetch();
        state.sqlite_diagnostics.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                quick_check: DiagnosticField::Pending,
                ..Default::default()
            },
        );

        let effects = reduce_at_boundary(&mut state, Action::RunSqliteDiagnosticsQuickCheck);

        assert!(state.sqlite_diagnostics.is_quick_check_running());
        assert!(matches!(
            effects.as_slice(),
            [Effect::FetchSqliteDiagnosticsQuickCheck { .. }]
        ));
    }

    #[test]
    fn open_is_a_noop_for_postgres_connection() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/db",
        );

        let effects =
            reduce_at_boundary(&mut state, Action::OpenModal(ModalKind::SqliteDiagnostics));

        assert_eq!(state.input_mode(), InputMode::Normal);
        assert!(effects.is_empty());
        assert!(state.messages.last_error.is_none());
    }

    #[test]
    fn quick_check_is_a_noop_for_postgres_connection() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/db",
        );
        let run_id = state.sqlite_diagnostics.begin_core_fetch();
        state.sqlite_diagnostics.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                quick_check: DiagnosticField::Pending,
                ..Default::default()
            },
        );

        let effects = reduce_at_boundary(&mut state, Action::RunSqliteDiagnosticsQuickCheck);

        assert!(effects.is_empty());
        assert!(!state.sqlite_diagnostics.is_quick_check_running());
        assert!(state.messages.last_error.is_none());
    }

    #[test]
    fn quick_check_is_ignored_for_postgres_connection() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/db",
        );

        let effects = reduce_sqlite_diagnostics(
            &mut state,
            &Action::RunSqliteDiagnosticsQuickCheck,
            Instant::now(),
        )
        .unwrap();

        assert!(effects.is_empty());
        assert!(!state.sqlite_diagnostics.is_quick_check_running());
    }

    #[test]
    fn close_cancels_diagnostics_task_after_clearing_modal_state() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        state.modal.set_mode(InputMode::SqliteDiagnostics);
        state.sqlite_diagnostics.begin_core_fetch();

        let effects =
            reduce_at_boundary(&mut state, Action::CloseModal(ModalKind::SqliteDiagnostics));

        assert_eq!(state.input_mode(), InputMode::Normal);
        assert!(state.sqlite_diagnostics.snapshot().is_none());
        assert!(matches!(
            effects.as_slice(),
            [Effect::CancelSqliteDiagnostics]
        ));
    }

    #[test]
    fn quick_check_loaded_ignores_stale_run_id() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        state.modal.set_mode(InputMode::SqliteDiagnostics);
        let stale_run_id = state.sqlite_diagnostics.begin_core_fetch();
        state.sqlite_diagnostics.set_core_loaded(
            stale_run_id,
            SqliteDiagnosticsSnapshot {
                quick_check: DiagnosticField::Pending,
                ..Default::default()
            },
        );
        let stale_quick_check_run_id = state
            .sqlite_diagnostics
            .begin_quick_check()
            .expect("loaded diagnostics should start quick check");
        state.sqlite_diagnostics.clear();
        let current_run_id = state.sqlite_diagnostics.begin_core_fetch();
        let current_snapshot = SqliteDiagnosticsSnapshot {
            quick_check: DiagnosticField::ok("current result"),
            ..Default::default()
        };
        state
            .sqlite_diagnostics
            .set_core_loaded(current_run_id, current_snapshot.clone());
        let quick_check_running_before = state.sqlite_diagnostics.is_quick_check_running();

        reduce_sqlite_diagnostics(
            &mut state,
            &Action::SqliteDiagnosticsQuickCheckLoaded {
                run_id: stale_quick_check_run_id,
                quick_check: DiagnosticField::ok("ok"),
            },
            Instant::now(),
        )
        .unwrap();

        assert!(stale_run_id < current_run_id);
        assert!(stale_quick_check_run_id < current_run_id);
        assert_eq!(state.sqlite_diagnostics.snapshot(), Some(&current_snapshot));
        assert_eq!(
            state.sqlite_diagnostics.is_quick_check_running(),
            quick_check_running_before
        );
        assert_eq!(state.input_mode(), InputMode::SqliteDiagnostics);
    }

    #[test]
    fn core_loaded_ignores_stale_run_id_after_new_fetch() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        state.modal.set_mode(InputMode::SqliteDiagnostics);
        let stale_run_id = state.sqlite_diagnostics.begin_core_fetch();
        state
            .sqlite_diagnostics
            .set_core_loaded(stale_run_id, SqliteDiagnosticsSnapshot::default());
        state.sqlite_diagnostics.clear();
        let current_run_id = state.sqlite_diagnostics.begin_core_fetch();

        reduce_sqlite_diagnostics(
            &mut state,
            &Action::SqliteDiagnosticsCoreLoaded {
                run_id: stale_run_id,
                snapshot: Box::new(SqliteDiagnosticsSnapshot::default()),
            },
            Instant::now(),
        )
        .unwrap();

        assert!(stale_run_id < current_run_id);
        assert!(state.sqlite_diagnostics.snapshot().is_none());
        assert!(state.sqlite_diagnostics.is_loading());
        assert!(!state.sqlite_diagnostics.is_quick_check_running());
        assert_eq!(state.input_mode(), InputMode::SqliteDiagnostics);
    }

    #[test]
    fn scroll_down_is_clamped_when_content_fits_viewport() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        state.sqlite_diagnostics.apply_viewport_metrics(5, 10);

        reduce_sqlite_diagnostics(
            &mut state,
            &Action::Scroll {
                target: ScrollTarget::SqliteDiagnostics,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            },
            Instant::now(),
        )
        .unwrap();

        assert_eq!(state.sqlite_diagnostics.scroll_offset(), 0);
    }
}
