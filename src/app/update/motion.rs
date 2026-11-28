use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::motion::ScrolledPane;
use crate::update::action::{Action, ScrollAmount, ScrollTarget, ScrollToCursorTarget};

/// What was on screen before an action, so transitions start from it.
pub(super) struct MotionProbe {
    focused_pane: FocusedPane,
    shows_overlay: bool,
    scroll_before: Option<(ScrolledPane, usize)>,
}

impl MotionProbe {
    pub(super) fn capture(state: &AppState, action: &Action) -> Self {
        Self {
            focused_pane: state.ui.focused_pane(),
            shows_overlay: state.input_mode().shows_overlay(),
            scroll_before: animated_scroll_pane(action)
                .map(|pane| (pane, scroll_offset(state, pane))),
        }
    }

    pub(super) fn record(self, state: &mut AppState, now: Instant) {
        let shows_overlay = state.input_mode().shows_overlay();
        // Switching between modals keeps the scrim, so only a modal opened over the panes fades in.
        if shows_overlay && !self.shows_overlay {
            state.motion.begin_modal(now);
        }
        // Pane borders are hidden under a modal, so a focus change there needs no transition.
        if !shows_overlay {
            state
                .motion
                .begin_focus(self.focused_pane, state.ui.focused_pane(), now);
        }
        if let Some((pane, before)) = self.scroll_before {
            let after = scroll_offset(state, pane);
            state.motion.begin_scroll(pane, before, after, now);
        }
    }
}

fn animated_scroll_pane(action: &Action) -> Option<ScrolledPane> {
    match action {
        // Line steps stay immediate because key repeat and the mouse wheel already move one row at a time.
        Action::Scroll { target, amount, .. } if *amount != ScrollAmount::Line => match target {
            ScrollTarget::Result => Some(ScrolledPane::Result),
            ScrollTarget::Inspector => Some(ScrolledPane::Inspector),
            _ => None,
        },
        Action::ScrollToCursor {
            target: ScrollToCursorTarget::Result,
            ..
        } => Some(ScrolledPane::Result),
        _ => None,
    }
}

fn scroll_offset(state: &AppState, pane: ScrolledPane) -> usize {
    match pane {
        ScrolledPane::Result => state.result_interaction.scroll_offset(),
        ScrolledPane::Inspector => state.ui.inspector_scroll_offset(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::model::shared::input_mode::InputMode;
    use crate::update::action::ScrollDirection;

    fn motion_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.motion.enable();
        state
    }

    fn page_down() -> Action {
        Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::HalfPage,
        }
    }

    #[test]
    fn opening_modal_over_panes_starts_fade() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::None);

        state.modal.set_mode(InputMode::Help);
        probe.record(&mut state, now);

        assert!(state.motion.modal_progress(now).is_some());
    }

    #[test]
    fn switching_between_modals_keeps_scrim_settled() {
        let mut state = motion_state();
        state.modal.set_mode(InputMode::CommandPalette);
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::None);

        state.modal.replace_mode(InputMode::Settings);
        probe.record(&mut state, now);

        assert_eq!(state.motion.modal_progress(now), None);
    }

    #[test]
    fn focus_change_in_browse_starts_transition() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::None);

        state.ui.set_focused_pane(FocusedPane::Result);
        probe.record(&mut state, now);

        let transition = state.motion.focus_transition(now).expect("transition");
        assert_eq!(transition.from, FocusedPane::Explorer);
        assert_eq!(transition.to, FocusedPane::Result);
    }

    #[test]
    fn focus_change_under_modal_skips_transition() {
        let mut state = motion_state();
        state.modal.set_mode(InputMode::Help);
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::None);

        state.ui.set_focused_pane(FocusedPane::Result);
        probe.record(&mut state, now);

        assert_eq!(state.motion.focus_transition(now), None);
    }

    #[test]
    fn page_scroll_animates_from_previous_offset() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());

        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);

        assert_eq!(state.motion.scroll_offset(ScrolledPane::Result, 20, now), 0);
        assert_eq!(
            state
                .motion
                .scroll_offset(ScrolledPane::Result, 20, now + Duration::from_secs(1)),
            20
        );
    }

    #[test]
    fn line_scroll_draws_new_offset_immediately() {
        let mut state = motion_state();
        let now = Instant::now();
        let line_down = Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        };
        let probe = MotionProbe::capture(&state, &line_down);

        state.result_interaction.set_scroll_offset(1);
        probe.record(&mut state, now);

        assert_eq!(state.motion.scroll_offset(ScrolledPane::Result, 1, now), 1);
    }
}
