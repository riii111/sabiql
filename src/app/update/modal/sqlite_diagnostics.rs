use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::policy::{FeaturePolicy, FeatureRequirement};
use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_sqlite_diagnostics(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::SqliteDiagnostics) => {
            let feature_policy = FeaturePolicy::new(state.session.active_engine_feature_profile());
            if !feature_policy.is_enabled(FeatureRequirement::SqliteDiagnostics) {
                state.messages.set_error_at(
                    "SQLite diagnostics are not available for this connection".to_string(),
                    now,
                );
                return DispatchResult::handled();
            }
            let Some(dsn) = state.session.dsn().map(String::from) else {
                return DispatchResult::handled();
            };
            let run_id = state.sqlite_diagnostics.begin_fetch();
            state.modal.set_mode(InputMode::SqliteDiagnostics);
            DispatchResult::handled_with(vec![Effect::FetchSqliteDiagnosticsCore { dsn, run_id }])
        }
        Action::RunSqliteDiagnosticsQuickCheck => {
            let feature_policy = FeaturePolicy::new(state.session.active_engine_feature_profile());
            if !feature_policy.is_enabled(FeatureRequirement::SqliteDiagnostics) {
                state.messages.set_error_at(
                    "SQLite diagnostics are not available for this connection".to_string(),
                    now,
                );
                return DispatchResult::handled();
            }
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
            DispatchResult::handled()
        }
        Action::SqliteDiagnosticsCoreLoaded {
            dsn,
            run_id,
            snapshot,
        } => {
            if !state.session.dsn_matches(dsn) || !state.sqlite_diagnostics.is_current_run(*run_id)
            {
                return DispatchResult::handled();
            }
            state
                .sqlite_diagnostics
                .set_core_loaded(*run_id, snapshot.as_ref().clone());
            DispatchResult::handled()
        }
        Action::SqliteDiagnosticsQuickCheckLoaded {
            dsn,
            run_id,
            quick_check,
        } => {
            if !state.session.dsn_matches(dsn) || !state.sqlite_diagnostics.is_current_run(*run_id)
            {
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
    use crate::update::test_fixtures;

    #[test]
    fn open_starts_split_fetch_for_sqlite_connection() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");

        let effects = reduce_sqlite_diagnostics(
            &mut state,
            &Action::OpenModal(ModalKind::SqliteDiagnostics),
            Instant::now(),
        )
        .unwrap();

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
        let run_id = state.sqlite_diagnostics.begin_fetch();
        state.sqlite_diagnostics.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                quick_check: DiagnosticField::Pending,
                ..Default::default()
            },
        );

        let effects = reduce_sqlite_diagnostics(
            &mut state,
            &Action::RunSqliteDiagnosticsQuickCheck,
            Instant::now(),
        )
        .unwrap();

        assert!(state.sqlite_diagnostics.is_quick_check_running());
        assert!(matches!(
            effects.as_slice(),
            [Effect::FetchSqliteDiagnosticsQuickCheck { .. }]
        ));
    }

    #[test]
    fn open_returns_error_for_postgres_connection() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/db",
        );

        let effects = reduce_sqlite_diagnostics(
            &mut state,
            &Action::OpenModal(ModalKind::SqliteDiagnostics),
            Instant::now(),
        )
        .unwrap();

        assert_eq!(state.input_mode(), InputMode::Normal);
        assert!(effects.is_empty());
        assert_eq!(
            state.messages.last_error.as_deref(),
            Some("SQLite diagnostics are not available for this connection")
        );
    }

    #[test]
    fn quick_check_returns_error_for_postgres_connection() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/db",
        );
        let run_id = state.sqlite_diagnostics.begin_fetch();
        state.sqlite_diagnostics.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                quick_check: DiagnosticField::Pending,
                ..Default::default()
            },
        );

        let effects = reduce_sqlite_diagnostics(
            &mut state,
            &Action::RunSqliteDiagnosticsQuickCheck,
            Instant::now(),
        )
        .unwrap();

        assert!(effects.is_empty());
        assert!(!state.sqlite_diagnostics.is_quick_check_running());
        assert_eq!(
            state.messages.last_error.as_deref(),
            Some("SQLite diagnostics are not available for this connection")
        );
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
    fn quick_check_loaded_ignores_stale_run_id() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        let run_id = state.sqlite_diagnostics.begin_fetch();
        state.sqlite_diagnostics.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot {
                quick_check: DiagnosticField::Pending,
                ..Default::default()
            },
        );

        reduce_sqlite_diagnostics(
            &mut state,
            &Action::SqliteDiagnosticsQuickCheckLoaded {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: run_id + 1,
                quick_check: DiagnosticField::ok("ok"),
            },
            Instant::now(),
        )
        .unwrap();

        assert!(
            state
                .sqlite_diagnostics
                .snapshot()
                .unwrap()
                .quick_check
                .is_pending()
        );
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
