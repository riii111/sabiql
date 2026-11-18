use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::{
    Action, CursorPosition, ListMotion, ListTarget, ModalKind, ScrollAmount, ScrollDirection,
    ScrollTarget, ScrollToCursorTarget, SelectMotion,
};
use crate::update::explain;
use crate::update::reducer::legacy_dispatch;

pub(in crate::update) fn scroll(
    state: &mut AppState,
    target: ScrollTarget,
    direction: ScrollDirection,
    amount: ScrollAmount,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    match target {
        ScrollTarget::ExplainPlan => explain::scroll::scroll_explain_plan(state, direction, amount),
        ScrollTarget::ExplainCompare => {
            explain::scroll::scroll_explain_compare(state, direction, amount)
        }
        ScrollTarget::ExplainConfirm => {
            explain::scroll::scroll_explain_confirm(state, direction, amount)
        }
        _ => legacy_dispatch(
            state,
            Action::Scroll {
                target,
                direction,
                amount,
            },
            now,
            services,
        ),
    }
}

pub(in crate::update) fn scroll_to_cursor(
    state: &mut AppState,
    target: ScrollToCursorTarget,
    position: CursorPosition,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(
        state,
        Action::ScrollToCursor { target, position },
        now,
        services,
    )
}

pub(in crate::update) fn select(
    state: &mut AppState,
    motion: SelectMotion,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(state, Action::Select(motion), now, services)
}

pub(in crate::update) fn list_select(
    state: &mut AppState,
    target: ListTarget,
    motion: ListMotion,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(state, Action::ListSelect { target, motion }, now, services)
}

pub(in crate::update) fn open_modal(
    state: &mut AppState,
    kind: ModalKind,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(state, Action::OpenModal(kind), now, services)
}

pub(in crate::update) fn close_modal(
    state: &mut AppState,
    kind: ModalKind,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(state, Action::CloseModal(kind), now, services)
}

pub(in crate::update) fn paste(
    state: &mut AppState,
    text: String,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(state, Action::Paste(text), now, services)
}
