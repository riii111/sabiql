use crate::model::app_state::AppState;
use crate::model::explain_context::ExplainContext;
use crate::update::action::{ScrollAmount, ScrollDirection};

pub(in crate::update) fn scroll_explain_confirm(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match amount {
        ScrollAmount::Line => {
            // blank + title + blank + separator + blank + warning(2) + blank = 8
            const CONFIRM_HEADER_LINES: usize = 8;
            let content_lines =
                CONFIRM_HEADER_LINES + state.sql_modal.editor.content().lines().count();
            let modal_inner = ExplainContext::modal_inner_height(state.ui.terminal_height);
            let max = content_lines.saturating_sub(modal_inner);
            let offset = &mut state.explain.confirm_scroll_offset;
            *offset = direction.clamp_vertical_offset(*offset, max, 1);
        }
        _ => debug_assert!(
            false,
            "unsupported ExplainConfirm scroll: {direction:?} {amount:?}"
        ),
    }
}

pub(in crate::update) fn scroll_explain_plan(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match amount {
        ScrollAmount::Line => {
            let modal_inner = ExplainContext::modal_inner_height(state.ui.terminal_height);
            let max = state.explain.line_count().saturating_sub(modal_inner);
            let offset = &mut state.explain.scroll_offset;
            *offset = direction.clamp_vertical_offset(*offset, max, 1);
        }
        _ => debug_assert!(
            false,
            "unsupported ExplainPlan scroll: {direction:?} {amount:?}"
        ),
    }
}

pub(in crate::update) fn scroll_explain_compare(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
) {
    match amount {
        ScrollAmount::Line => {
            let max = state.explain.compare_max_scroll(state.ui.terminal_height);
            let offset = &mut state.explain.compare_scroll_offset;
            *offset = direction.clamp_vertical_offset(*offset, max, 1);
        }
        _ => debug_assert!(
            false,
            "unsupported ExplainCompare scroll: {direction:?} {amount:?}"
        ),
    }
}
