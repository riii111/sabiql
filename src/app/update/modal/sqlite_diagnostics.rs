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
            if !state
                .session
                .active_db_capabilities()
                .supports_sqlite_diagnostics()
            {
                return DispatchResult::handled();
            }
            let Some(dsn) = state.session.dsn().map(String::from) else {
                return DispatchResult::handled();
            };
            let run_id = state.sqlite_diagnostics.begin_fetch();
            state.modal.set_mode(InputMode::SqliteDiagnostics);
            let read_only = state.session.is_read_only();
            DispatchResult::handled_with(vec![
                Effect::FetchSqliteDiagnosticsCore {
                    dsn: dsn.clone(),
                    run_id,
                    read_only,
                },
                Effect::FetchSqliteDiagnosticsQuickCheck {
                    dsn,
                    run_id,
                    read_only,
                },
            ])
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
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            effects[0],
            Effect::FetchSqliteDiagnosticsCore { .. }
        ));
        assert!(matches!(
            effects[1],
            Effect::FetchSqliteDiagnosticsQuickCheck { .. }
        ));
    }

    #[test]
    fn open_is_ignored_for_postgres_connection() {
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
    }

    #[test]
    fn quick_check_loaded_ignores_stale_run_id() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        let run_id = state.sqlite_diagnostics.begin_fetch();
        state
            .sqlite_diagnostics
            .set_core_loaded(run_id, SqliteDiagnosticsSnapshot::default());

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

        assert!(state.sqlite_diagnostics.is_quick_check_pending());
    }

    #[test]
    fn quick_check_loaded_before_core_clears_pending_when_core_arrives() {
        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        let run_id = state.sqlite_diagnostics.begin_fetch();

        reduce_sqlite_diagnostics(
            &mut state,
            &Action::SqliteDiagnosticsQuickCheckLoaded {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id,
                quick_check: DiagnosticField::ok("ok"),
            },
            Instant::now(),
        )
        .unwrap();

        assert!(state.sqlite_diagnostics.is_loading());

        reduce_sqlite_diagnostics(
            &mut state,
            &Action::SqliteDiagnosticsCoreLoaded {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id,
                snapshot: Box::new(SqliteDiagnosticsSnapshot {
                    sqlite_version: DiagnosticField::ok("3.45.0"),
                    ..Default::default()
                }),
            },
            Instant::now(),
        )
        .unwrap();

        assert!(!state.sqlite_diagnostics.is_quick_check_pending());
        assert_eq!(
            state
                .sqlite_diagnostics
                .snapshot()
                .unwrap()
                .quick_check
                .ok_value(),
            Some("ok")
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
