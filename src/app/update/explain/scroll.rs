use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::explain_context::ExplainContext;
use crate::model::shared::text_input::TextInputLike;
use crate::update::action::{Action, ScrollAmount, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_scroll(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::Scroll {
            target:
                target @ (ScrollTarget::ExplainConfirm
                | ScrollTarget::ExplainPlan
                | ScrollTarget::ExplainCompare),
            direction,
            amount: ScrollAmount::Line,
        } => {
            let (offset, max) = match target {
                ScrollTarget::ExplainConfirm => {
                    // blank + title + blank + separator + blank + warning(2) + blank = 8
                    const CONFIRM_HEADER_LINES: usize = 8;
                    let content_lines =
                        CONFIRM_HEADER_LINES + state.sql_modal.editor.content().lines().count();
                    let modal_inner = ExplainContext::modal_inner_height(state.ui.terminal_height);
                    (
                        &mut state.explain.confirm_scroll_offset,
                        content_lines.saturating_sub(modal_inner),
                    )
                }
                ScrollTarget::ExplainPlan => {
                    let modal_inner = ExplainContext::modal_inner_height(state.ui.terminal_height);
                    let max = state.explain.line_count().saturating_sub(modal_inner);
                    (&mut state.explain.scroll_offset, max)
                }
                ScrollTarget::ExplainCompare => {
                    let max = state.explain.compare_max_scroll(state.ui.terminal_height);
                    (&mut state.explain.compare_scroll_offset, max)
                }
                _ => unreachable!(),
            };
            *offset = direction.clamp_vertical_offset(*offset, max, 1);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
