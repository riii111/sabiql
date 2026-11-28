use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::motion::ScrolledPane;
use crate::update::action::{
    Action, ScrollAmount, ScrollTarget, ScrollToCursorTarget, SelectMotion,
};

/// What was on screen before an action, so transitions start from it.
pub(super) struct MotionProbe {
    focused_pane: FocusedPane,
    animating_view: Option<(ScrolledPane, PaneView)>,
    focus_mode: bool,
    shows_overlay: bool,
    scroll_before: Option<(ScrolledPane, usize)>,
}

impl MotionProbe {
    pub(super) fn capture(state: &AppState, action: &Action) -> Self {
        Self {
            focused_pane: state.ui.focused_pane(),
            animating_view: state
                .motion
                .scrolling_pane()
                .map(|pane| (pane, pane_view(state, pane))),
            focus_mode: state.ui.is_focus_mode(),
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
        if !shows_overlay && self.shows_overlay {
            state.motion.begin_modal_close(now);
        }
        let focus_mode = state.ui.is_focus_mode();
        if focus_mode != self.focus_mode {
            state.motion.begin_focus_mode(focus_mode, now);
        }
        // Pane borders are hidden under a modal, so a focus change there needs no transition.
        if !shows_overlay {
            state
                .motion
                .begin_focus(self.focused_pane, state.ui.focused_pane(), now);
        }
        // A change to the animating pane must show at once unless a new page jump retargets it; unrelated updates keep it running.
        let retargets = self
            .scroll_before
            .filter(|&(pane, before)| scroll_offset(state, pane) != before)
            .map(|(pane, _)| pane);
        if let Some((pane, view)) = self.animating_view
            && Some(pane) != retargets
            && pane_view(state, pane) != view
        {
            state.motion.cancel_scroll();
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
            ScrollTarget::Help => Some(ScrolledPane::Help),
            ScrollTarget::ExplainPlan => Some(ScrolledPane::ExplainPlan),
            ScrollTarget::ExplainCompare => Some(ScrolledPane::ExplainCompare),
            ScrollTarget::SqliteDiagnostics => Some(ScrolledPane::SqliteDiagnostics),
            ScrollTarget::CellDetail => Some(ScrolledPane::CellDetail),
            ScrollTarget::RowDetail => Some(ScrolledPane::RowDetail),
            // Short dialogs rarely scroll, and the explorer list moves through its selection instead.
            ScrollTarget::ConnectionError
            | ScrollTarget::ConfirmDialog
            | ScrollTarget::ExplainConfirm
            | ScrollTarget::Explorer => None,
        },
        Action::ScrollToCursor { target, .. } => Some(match target {
            ScrollToCursorTarget::Result => ScrolledPane::Result,
            ScrollToCursorTarget::Explorer => ScrolledPane::Explorer,
        }),
        Action::Select(motion)
            if !matches!(motion, SelectMotion::Next | SelectMotion::Previous) =>
        {
            Some(ScrolledPane::Explorer)
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PaneView {
    offset: usize,
    selection: Option<(Option<usize>, Option<usize>)>,
}

fn pane_view(state: &AppState, pane: ScrolledPane) -> PaneView {
    let selection = match pane {
        ScrolledPane::Explorer => Some((Some(state.ui.explorer_selected()), None)),
        ScrolledPane::Result => {
            let selection = state.result_interaction.selection();
            Some((selection.row(), selection.cell()))
        }
        ScrolledPane::Inspector
        | ScrolledPane::Help
        | ScrolledPane::ExplainPlan
        | ScrolledPane::ExplainCompare
        | ScrolledPane::SqliteDiagnostics
        | ScrolledPane::CellDetail
        | ScrolledPane::RowDetail => None,
    };
    PaneView {
        offset: scroll_offset(state, pane),
        selection,
    }
}

fn scroll_offset(state: &AppState, pane: ScrolledPane) -> usize {
    match pane {
        ScrolledPane::Result => state.result_interaction.scroll_offset(),
        ScrolledPane::Inspector => state.ui.inspector_scroll_offset(),
        ScrolledPane::Explorer => state.ui.explorer_scroll_offset(),
        ScrolledPane::Help => state.ui.help().scroll_offset(),
        ScrolledPane::ExplainPlan => state.explain.scroll_offset(),
        ScrolledPane::ExplainCompare => state.explain.compare_scroll_offset(),
        ScrolledPane::SqliteDiagnostics => state.sqlite_diagnostics.scroll_offset(),
        ScrolledPane::CellDetail => state.cell_detail.scroll_offset(),
        ScrolledPane::RowDetail => state.row_detail.scroll_offset(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::shared::ui_state::FocusMode;
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
    fn closing_modal_to_panes_starts_fade_out() {
        let mut state = motion_state();
        state.modal.set_mode(InputMode::Help);
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::None);

        state.modal.set_mode(InputMode::Normal);
        probe.record(&mut state, now);

        assert_eq!(state.motion.modal_close_visibility(now), Some(1.0));
    }

    #[test]
    fn entering_focus_mode_starts_layout_transition() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::None);

        state.ui.set_focus_mode(FocusMode::Focused {
            previous_pane: FocusedPane::Explorer,
        });
        probe.record(&mut state, now);

        assert_eq!(state.motion.focus_mode_collapse(now), Some(0.0));
    }

    #[test]
    fn explorer_page_jump_animates_list_window() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::Select(SelectMotion::FullPageDown));

        state.ui.set_explorer_scroll_offset(20);
        probe.record(&mut state, now);

        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Explorer, 20, now),
            0
        );
    }

    #[test]
    fn help_page_scroll_animates() {
        let mut state = motion_state();
        state.modal.set_mode(InputMode::Help);
        let now = Instant::now();
        let action = Action::Scroll {
            target: ScrollTarget::Help,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::HalfPage,
        };
        let probe = MotionProbe::capture(&state, &action);

        state.ui.help_mut().set_scroll_offset(12);
        probe.record(&mut state, now);

        assert_eq!(state.motion.scroll_offset(ScrolledPane::Help, 12, now), 0);
    }

    #[test]
    fn second_page_jump_mid_animation_continues_from_drawn_offset() {
        let mut state = motion_state();
        let start = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, start);
        let midway = start + Duration::from_millis(40);
        let drawn = state.motion.scroll_offset(ScrolledPane::Result, 20, midway);

        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(40);
        probe.record(&mut state, midway);

        assert!(drawn < 20, "drawn offset was {drawn}");
        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Result, 40, midway),
            drawn
        );
    }

    #[test]
    fn page_jump_retargets_from_last_drawn_offset() {
        let mut state = motion_state();
        let start = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, start);
        let drawn_at = start + Duration::from_millis(30);
        state.motion.note_frame(drawn_at);
        let drawn = state
            .motion
            .scroll_offset(ScrolledPane::Result, 20, drawn_at);
        let pressed_at = start + Duration::from_millis(90);

        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(40);
        probe.record(&mut state, pressed_at);

        assert_eq!(
            state
                .motion
                .scroll_offset(ScrolledPane::Result, 40, pressed_at),
            drawn
        );
    }

    #[test]
    fn line_scroll_after_page_jump_ends_the_animation() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);
        let line_up = Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        };

        let probe = MotionProbe::capture(&state, &line_up);
        state.result_interaction.set_scroll_offset(19);
        probe.record(&mut state, now);
        let probe = MotionProbe::capture(&state, &line_up);
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);

        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Result, 20, now),
            20
        );
    }

    #[test]
    fn selection_step_during_page_animation_shows_at_once() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::Select(SelectMotion::FullPageDown));
        state.ui.set_explorer_scroll_offset(20);
        probe.record(&mut state, now);

        let probe = MotionProbe::capture(&state, &Action::Select(SelectMotion::Next));
        state.ui.set_explorer_selection(Some(1));
        state.ui.set_explorer_scroll_offset(20);
        probe.record(&mut state, now);

        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Explorer, 20, now),
            20
        );
    }

    #[test]
    fn selection_jump_within_the_page_shows_at_once() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &Action::Select(SelectMotion::FullPageDown));
        state.ui.set_explorer_scroll_offset(20);
        probe.record(&mut state, now);

        let probe = MotionProbe::capture(&state, &Action::Select(SelectMotion::ViewportBottom));
        state.ui.set_explorer_selection(Some(25));
        state.ui.set_explorer_scroll_offset(20);
        probe.record(&mut state, now);

        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Explorer, 20, now),
            20
        );
    }

    #[test]
    fn background_update_keeps_the_scroll_animation() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);

        let probe = MotionProbe::capture(&state, &Action::None);
        probe.record(&mut state, now);

        assert_eq!(state.motion.scroll_offset(ScrolledPane::Result, 20, now), 0);
    }

    #[test]
    fn animation_tick_keeps_the_scroll_animation() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);

        let probe = MotionProbe::capture(&state, &Action::Render);
        probe.record(&mut state, now);

        assert_eq!(state.motion.scroll_offset(ScrolledPane::Result, 20, now), 0);
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
