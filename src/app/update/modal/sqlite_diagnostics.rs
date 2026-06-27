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
            DispatchResult::handled_with(vec![Effect::FetchSqliteDiagnostics {
                dsn,
                run_id,
                read_only: state.session.is_read_only(),
            }])
        }
        Action::CloseModal(ModalKind::SqliteDiagnostics) => {
            state.sqlite_diagnostics.clear();
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }
        Action::SqliteDiagnosticsLoaded {
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
                .set_loaded(*run_id, snapshot.as_ref().clone());
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
            let max_scroll = state.sqlite_diagnostics.line_count().saturating_sub(1);
            state.sqlite_diagnostics.scroll_down(max_scroll);
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
    use crate::update::test_support::activate_sqlite_connection;

    #[test]
    fn open_starts_fetch_for_sqlite_connection() {
        let mut state = AppState::new("test".to_string());
        activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");

        let effects = reduce_sqlite_diagnostics(
            &mut state,
            &Action::OpenModal(ModalKind::SqliteDiagnostics),
            Instant::now(),
        )
        .unwrap();

        assert_eq!(state.input_mode(), InputMode::SqliteDiagnostics);
        assert!(matches!(
            effects.first(),
            Some(Effect::FetchSqliteDiagnostics { .. })
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
    fn loaded_snapshot_ignores_stale_run_id() {
        let mut state = AppState::new("test".to_string());
        activate_sqlite_connection(&mut state, "sqlite:///tmp/app.db");
        let run_id = state.sqlite_diagnostics.begin_fetch();

        reduce_sqlite_diagnostics(
            &mut state,
            &Action::SqliteDiagnosticsLoaded {
                dsn: "sqlite:///tmp/app.db".to_string(),
                run_id: run_id + 1,
                snapshot: Box::new(SqliteDiagnosticsSnapshot {
                    sqlite_version: DiagnosticField::ok("9.9.9"),
                    ..Default::default()
                }),
            },
            Instant::now(),
        )
        .unwrap();

        assert!(state.sqlite_diagnostics.snapshot().is_none());
    }
}
