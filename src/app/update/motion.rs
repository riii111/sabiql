use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::motion::ScrolledPane;
use crate::update::action::{Action, ScrollAmount, ScrollTarget};

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
            scroll_before: animated_scroll_pane(state, action)
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
        // Pane borders are hidden under a modal, so a focus change there needs no transition.
        if !shows_overlay {
            state
                .motion
                .begin_focus(self.focused_pane, state.ui.focused_pane(), now);
        }
        // A scroll animation only ever shows the way to the confirmed offset, so any other change to that pane ends it.
        if let Some((pane, target)) = state.motion.scroll_target()
            && (scroll_offset(state, pane) != target || !is_animatable(state, pane))
        {
            state.motion.cancel_scroll();
        }
        if let Some((pane, before)) = self.scroll_before {
            let after = scroll_offset(state, pane);
            state.motion.begin_scroll(pane, before, after, now);
        }
    }
}

fn animated_scroll_pane(state: &AppState, action: &Action) -> Option<ScrolledPane> {
    let pane = match action {
        // Line steps stay immediate because key repeat and the mouse wheel already move one row at a time.
        Action::Scroll { target, amount, .. } if *amount != ScrollAmount::Line => {
            scrolled_pane(*target)?
        }
        _ => return None,
    };
    is_animatable(state, pane).then_some(pane)
}

fn scrolled_pane(target: ScrollTarget) -> Option<ScrolledPane> {
    Some(match target {
        ScrollTarget::Result => ScrolledPane::Result,
        ScrollTarget::Inspector => ScrolledPane::Inspector,
        ScrollTarget::Help => ScrolledPane::Help,
        ScrollTarget::ExplainPlan => ScrolledPane::ExplainPlan,
        ScrollTarget::ExplainCompare => ScrolledPane::ExplainCompare,
        ScrollTarget::SqliteDiagnostics => ScrolledPane::SqliteDiagnostics,
        ScrollTarget::CellDetail => ScrolledPane::CellDetail,
        ScrollTarget::RowDetail => ScrolledPane::RowDetail,
        // Short dialogs rarely scroll, and the explorer keeps a selection that must move with its window.
        ScrollTarget::ConnectionError
        | ScrollTarget::ConfirmDialog
        | ScrollTarget::ExplainConfirm
        | ScrollTarget::Explorer => return None,
    })
}

/// Moves an animating pane to the offset last drawn before a line step, so the step starts from what is on screen.
pub(super) fn settle_before_line_step(state: &mut AppState, action: &Action) {
    let Action::Scroll {
        target,
        amount: ScrollAmount::Line,
        ..
    } = action
    else {
        return;
    };
    let Some((pane, target_offset)) = state.motion.scroll_target() else {
        return;
    };
    if scrolled_pane(*target) != Some(pane) {
        return;
    }
    let drawn = state.motion.drawn_scroll_offset(pane, target_offset);
    match pane {
        ScrolledPane::Result => state.result_interaction.set_scroll_offset(drawn),
        ScrolledPane::Inspector => state.ui.inspector_scroll_offset = drawn,
        ScrolledPane::Help => state.ui.help_mut().set_scroll_offset(drawn),
        ScrolledPane::SqliteDiagnostics => state.sqlite_diagnostics.set_scroll_offset(drawn),
        // These views do not expose their offset, so a line step there still starts from the page jump's target.
        ScrolledPane::ExplainPlan
        | ScrolledPane::ExplainCompare
        | ScrolledPane::CellDetail
        | ScrolledPane::RowDetail => {}
    }
}

// A selected row or cell must stay in view on every frame, so the result pane animates only while nothing is selected.
fn is_animatable(state: &AppState, pane: ScrolledPane) -> bool {
    pane != ScrolledPane::Result || state.result_interaction.selection().row().is_none()
}

fn scroll_offset(state: &AppState, pane: ScrolledPane) -> usize {
    match pane {
        ScrolledPane::Result => state.result_interaction.scroll_offset(),
        ScrolledPane::Inspector => state.ui.inspector_scroll_offset,
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
    use std::sync::Arc;

    use crate::domain::{QueryResult, QuerySource};
    use crate::model::shared::input_mode::InputMode;
    use crate::services::AppServices;
    use crate::update::action::ScrollDirection;
    use crate::update::reducer::reduce;

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

        state.focus_pane(FocusedPane::Result);
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

        state.focus_pane(FocusedPane::Result);
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

        assert_eq!(state.motion.modal_close_dim(now), Some(1.0));
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
    fn page_jump_with_a_selected_cell_moves_at_once() {
        let mut state = motion_state();
        state.result_interaction.activate_cell(0, 0);
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());

        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);

        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Result, 20, now),
            20
        );
    }

    #[test]
    fn selecting_a_cell_during_page_animation_ends_it() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);

        let probe = MotionProbe::capture(&state, &Action::None);
        state.result_interaction.activate_cell(25, 0);
        probe.record(&mut state, now);

        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Result, 20, now),
            20
        );
    }

    #[test]
    fn second_page_jump_starts_from_confirmed_offset() {
        let mut state = motion_state();
        let start = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, start);
        let midway = start + Duration::from_millis(40);

        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(40);
        probe.record(&mut state, midway);

        assert_eq!(
            state.motion.scroll_offset(ScrolledPane::Result, 40, midway),
            20
        );
    }

    #[test]
    fn selecting_a_cell_mid_animation_starts_at_the_drawn_top_row() {
        let mut state = motion_state();
        let rows = (0..60).map(|i| vec![i.to_string()]).collect();
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                "SELECT id".to_string(),
                vec!["id".to_string()],
                rows,
                1,
                QuerySource::Preview,
            )));
        let start = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, start);
        let drawn_at = start + Duration::from_millis(30);
        state.motion.note_frame(drawn_at);
        let drawn = state
            .motion
            .scroll_offset(ScrolledPane::Result, 20, drawn_at);

        reduce(
            &mut state,
            Action::ResultActivateCell,
            drawn_at,
            &AppServices::stub(),
        );

        assert!(drawn < 20, "drawn offset was {drawn}");
        assert_eq!(state.result_interaction.selection().row(), Some(drawn));
        assert_eq!(
            state
                .motion
                .scroll_offset(ScrolledPane::Result, drawn, drawn_at),
            drawn
        );
    }

    #[test]
    fn animation_tick_keeps_the_scroll_animation() {
        let mut state = motion_state();
        let now = Instant::now();
        let probe = MotionProbe::capture(&state, &page_down());
        state.result_interaction.set_scroll_offset(20);
        probe.record(&mut state, now);

        reduce(&mut state, Action::Render, now, &AppServices::stub());

        assert_eq!(state.motion.scroll_offset(ScrolledPane::Result, 20, now), 0);
    }

    #[test]
    fn line_step_mid_animation_moves_from_the_drawn_offset() {
        let mut state = motion_state();
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                "SELECT id".to_string(),
                vec!["id".to_string()],
                (0..60).map(|i| vec![i.to_string()]).collect(),
                1,
                QuerySource::Preview,
            )));
        state.ui.set_result_pane_height(24);
        let start = Instant::now();
        reduce(&mut state, page_down(), start, &AppServices::stub());
        let target = state.result_interaction.scroll_offset();
        let drawn_at = start + Duration::from_millis(30);
        state.motion.note_frame(drawn_at);
        let drawn = state
            .motion
            .scroll_offset(ScrolledPane::Result, target, drawn_at);
        let line_up = Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        };

        reduce(&mut state, line_up, drawn_at, &AppServices::stub());

        assert!(
            drawn > 0 && drawn < target,
            "drawn {drawn}, target {target}"
        );
        assert_eq!(state.result_interaction.scroll_offset(), drawn - 1);
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
