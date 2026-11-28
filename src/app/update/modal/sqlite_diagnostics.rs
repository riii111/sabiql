use crate::cmd::effect::Effect;
use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{ScrollAmount, ScrollDirection};

pub(in crate::update) fn open_modal_sqlite_diagnostics(state: &mut AppState) -> Vec<Effect> {
    let Some(endpoint) = state.session.endpoint().cloned() else {
        return vec![];
    };
    let run_id = state.sqlite_diagnostics.begin_core_fetch();
    state.modal.set_mode(InputMode::SqliteDiagnostics);
    vec![Effect::FetchSqliteDiagnosticsCore { endpoint, run_id }]
}

pub(in crate::update) fn run_sqlite_diagnostics_quick_check(state: &mut AppState) -> Vec<Effect> {
    let Some(endpoint) = state.session.endpoint().cloned() else {
        return vec![];
    };
    let Some(run_id) = state.sqlite_diagnostics.begin_quick_check() else {
        return vec![];
    };
    vec![Effect::FetchSqliteDiagnosticsQuickCheck { endpoint, run_id }]
}

pub(in crate::update) fn close_modal_sqlite_diagnostics(state: &mut AppState) -> Vec<Effect> {
    state.sqlite_diagnostics.clear();
    state.modal.set_mode(InputMode::Normal);
    vec![Effect::CancelSqliteDiagnostics]
}

pub(in crate::update) fn sqlite_diagnostics_core_loaded(
    state: &mut AppState,
    run_id: u64,
    snapshot: Box<SqliteDiagnosticsSnapshot>,
) -> Vec<Effect> {
    if !state.sqlite_diagnostics.is_current_run(run_id) {
        return vec![];
    }
    state
        .sqlite_diagnostics
        .set_core_loaded(run_id, snapshot.as_ref().clone());
    vec![]
}

pub(in crate::update) fn sqlite_diagnostics_quick_check_loaded(
    state: &mut AppState,
    run_id: u64,
    quick_check: DiagnosticField,
) -> Vec<Effect> {
    if !state.sqlite_diagnostics.is_current_run(run_id) {
        return vec![];
    }
    state
        .sqlite_diagnostics
        .set_quick_check_loaded(run_id, quick_check);
    vec![]
}

pub(in crate::update) fn scroll_sqlite_diagnostics(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match (direction, amount) {
        (ScrollDirection::Up, ScrollAmount::Line) => {
            state.sqlite_diagnostics.scroll_up();
        }
        (ScrollDirection::Down, ScrollAmount::Line) => {
            state.sqlite_diagnostics.scroll_down();
        }
        _ => debug_assert!(
            false,
            "unsupported SqliteDiagnostics scroll: {direction:?} {amount:?}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::domain::ConnectionId;

    use crate::services::AppServices;
    use crate::test_support::endpoint;
    use crate::update::action::{Action, ModalKind, ScrollTarget};
    use crate::update::reducer::{reduce, route};
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
        state.session.activate_connection(
            &ConnectionId::new(),
            "database",
            &endpoint::postgres("postgres://localhost/db"),
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
        state.session.activate_connection(
            &ConnectionId::new(),
            "database",
            &endpoint::postgres("postgres://localhost/db"),
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

        route(
            &mut state,
            Action::SqliteDiagnosticsQuickCheckLoaded {
                run_id: stale_quick_check_run_id,
                quick_check: DiagnosticField::ok("ok"),
            },
            Instant::now(),
            &AppServices::stub(),
        );

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

        route(
            &mut state,
            Action::SqliteDiagnosticsCoreLoaded {
                run_id: stale_run_id,
                snapshot: Box::new(SqliteDiagnosticsSnapshot::default()),
            },
            Instant::now(),
            &AppServices::stub(),
        );

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

        route(
            &mut state,
            Action::Scroll {
                target: ScrollTarget::SqliteDiagnostics,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            },
            Instant::now(),
            &AppServices::stub(),
        );

        assert_eq!(state.sqlite_diagnostics.scroll_offset(), 0);
    }
}
