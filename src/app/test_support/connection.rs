use crate::model::app_state::AppState;

pub fn assert_explain_state_cleared(state: &AppState) {
    assert!(state.explain.plan_text().is_none());
    assert!(state.explain.error().is_none());
    assert!(state.explain.left().is_none());
    assert!(state.explain.right().is_none());
    assert!(state.explain.history().is_empty());
}

pub fn assert_sqlite_diagnostics_cleared(state: &AppState) {
    assert!(state.sqlite_diagnostics.snapshot().is_none());
    assert!(!state.sqlite_diagnostics.is_loading());
    assert!(!state.sqlite_diagnostics.is_quick_check_running());
}
