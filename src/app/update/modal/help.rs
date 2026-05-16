use std::time::Instant;

use crate::catalog::HelpDocument;
use crate::model::app_state::AppState;
use crate::model::shared::help::HelpOrigin;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{
    Action, InputTarget, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
};
use crate::update::dispatch_result::DispatchResult;

fn scroll_help_by(
    state: &mut AppState,
    direction: ScrollDirection,
    delta: usize,
    line_count: usize,
    content_width: usize,
) {
    let max_scroll = state.ui.help_max_scroll(line_count, content_width);
    let offset = direction.clamp_vertical_offset(state.ui.help.scroll_offset(), max_scroll, delta);
    state.ui.help.set_scroll_offset(offset);
}

fn scroll_help_horizontally(state: &mut AppState, direction: ScrollDirection) {
    match direction {
        ScrollDirection::Left => {
            state
                .ui
                .help
                .set_horizontal_offset(state.ui.help.horizontal_offset().saturating_sub(1));
        }
        ScrollDirection::Right => {
            let document = HelpDocument::from_state(state);
            let max_scroll = state
                .ui
                .help_max_horizontal_scroll(document.line_count(), document.content_width());
            state
                .ui
                .help
                .set_horizontal_offset((state.ui.help.horizontal_offset() + 1).min(max_scroll));
        }
        ScrollDirection::Up | ScrollDirection::Down => {}
    }
}

fn close_help(state: &mut AppState) {
    state.modal.pop_mode();
    state.ui.help.close();
}

pub(super) fn reduce_help(state: &mut AppState, action: &Action, _now: Instant) -> DispatchResult {
    match action {
        Action::ToggleModal(ModalKind::Help) => {
            if state.modal.active_mode() == InputMode::Help {
                close_help(state);
            } else {
                let origin = HelpOrigin::from_state(state);
                state.ui.help.open(origin);
                state.modal.push_mode(InputMode::Help);
            }
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::Help) => {
            close_help(state);
            DispatchResult::handled()
        }
        Action::TextInput {
            target: InputTarget::HelpFilter,
            ch,
        } => {
            state.ui.help.insert_filter_char(*ch);
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::HelpFilter,
        } => {
            state.ui.help.backspace_filter();
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::Help,
            direction,
            amount,
        } => {
            match amount {
                ScrollAmount::Line
                    if matches!(direction, ScrollDirection::Left | ScrollDirection::Right) =>
                {
                    scroll_help_horizontally(state, *direction);
                }
                ScrollAmount::Line => {
                    let document = HelpDocument::from_state(state);
                    scroll_help_by(
                        state,
                        *direction,
                        1,
                        document.line_count(),
                        document.content_width(),
                    );
                }
                ScrollAmount::ToStart => {
                    if matches!(direction, ScrollDirection::Left | ScrollDirection::Right) {
                        state.ui.help.set_horizontal_offset(0);
                    } else {
                        state.ui.help.set_scroll_offset(0);
                    }
                }
                ScrollAmount::ToEnd => {
                    let document = HelpDocument::from_state(state);
                    if matches!(direction, ScrollDirection::Left | ScrollDirection::Right) {
                        let max_scroll = state.ui.help_max_horizontal_scroll(
                            document.line_count(),
                            document.content_width(),
                        );
                        state.ui.help.set_horizontal_offset(max_scroll);
                    } else {
                        let max_scroll = state
                            .ui
                            .help_max_scroll(document.line_count(), document.content_width());
                        state.ui.help.set_scroll_offset(max_scroll);
                    }
                }
                ScrollAmount::HalfPage | ScrollAmount::FullPage => {
                    let document = HelpDocument::from_state(state);
                    let line_count = document.line_count();
                    let content_width = document.content_width();
                    if let Some(delta) =
                        amount.page_delta(state.ui.help_visible_rows(line_count, content_width))
                    {
                        scroll_help_by(state, *direction, delta, line_count, content_width);
                    }
                }
                ScrollAmount::ViewportTop
                | ScrollAmount::ViewportMiddle
                | ScrollAmount::ViewportBottom => {}
            }
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
