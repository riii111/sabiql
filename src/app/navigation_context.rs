use super::focused_pane::FocusedPane;
use super::key_sequence::{KeySequenceState, Prefix};
use super::state::AppState;
use super::ui_state::ResultNavMode;

pub struct NavigationContext {
    pub active_pane: FocusedPane,
    pub focus_mode: bool,
    pub result_nav_mode: ResultNavMode,
    pub history_mode: bool,
    pub key_sequence: KeySequenceState,
}

impl NavigationContext {
    pub fn from_state(state: &AppState) -> Self {
        let active_pane = if state.ui.focus_mode {
            FocusedPane::Result
        } else {
            state.ui.focused_pane
        };
        Self {
            active_pane,
            focus_mode: state.ui.focus_mode,
            result_nav_mode: state.result_interaction.selection().mode(),
            history_mode: state.query.is_history_mode(),
            key_sequence: state.ui.key_sequence,
        }
    }

    pub fn result_navigation(&self) -> bool {
        self.active_pane == FocusedPane::Result
    }

    pub fn inspector_navigation(&self) -> bool {
        self.active_pane == FocusedPane::Inspector
    }

    pub fn explorer_navigation(&self) -> bool {
        self.active_pane == FocusedPane::Explorer
    }

    pub fn pending_prefix(&self) -> Option<Prefix> {
        self.key_sequence.pending_prefix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_state() -> AppState {
        AppState::new("test".to_string())
    }

    #[test]
    fn default_state_is_explorer() {
        let state = default_state();
        let ctx = NavigationContext::from_state(&state);

        assert_eq!(ctx.active_pane, FocusedPane::Explorer);
        assert!(ctx.explorer_navigation());
        assert!(!ctx.result_navigation());
        assert!(!ctx.inspector_navigation());
        assert!(!ctx.focus_mode);
        assert!(!ctx.history_mode);
        assert_eq!(ctx.pending_prefix(), None);
    }

    #[test]
    fn result_focused() {
        let mut state = default_state();
        state.ui.focused_pane = FocusedPane::Result;
        let ctx = NavigationContext::from_state(&state);

        assert!(ctx.result_navigation());
        assert!(!ctx.inspector_navigation());
        assert!(!ctx.explorer_navigation());
    }

    #[test]
    fn inspector_focused() {
        let mut state = default_state();
        state.ui.focused_pane = FocusedPane::Inspector;
        let ctx = NavigationContext::from_state(&state);

        assert!(ctx.inspector_navigation());
        assert!(!ctx.result_navigation());
        assert!(!ctx.explorer_navigation());
    }

    #[test]
    fn focus_mode_overrides_to_result() {
        let mut state = default_state();
        state.ui.focused_pane = FocusedPane::Explorer;
        state.ui.focus_mode = true;
        let ctx = NavigationContext::from_state(&state);

        assert_eq!(ctx.active_pane, FocusedPane::Result);
        assert!(ctx.result_navigation());
        assert!(ctx.focus_mode);
    }

    #[test]
    fn history_mode_reflected() {
        let mut state = default_state();
        state.query.enter_history(0);
        let ctx = NavigationContext::from_state(&state);

        assert!(ctx.history_mode);
    }

    #[test]
    fn result_nav_mode_defaults_to_scroll() {
        let state = default_state();
        let ctx = NavigationContext::from_state(&state);

        assert_eq!(ctx.result_nav_mode, ResultNavMode::Scroll);
    }

    #[test]
    fn result_nav_mode_reflects_row_active() {
        let mut state = default_state();
        state.result_interaction.enter_row(0);
        let ctx = NavigationContext::from_state(&state);

        assert_eq!(ctx.result_nav_mode, ResultNavMode::RowActive);
    }

    #[test]
    fn pending_z_reflected() {
        let mut state = default_state();
        state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);
        let ctx = NavigationContext::from_state(&state);

        assert_eq!(ctx.pending_prefix(), Some(Prefix::Z));
    }
}
