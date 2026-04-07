pub(super) mod browse;
pub(super) mod jsonb;
pub(super) mod sql;

use crate::app::update::action::{
    Action, CursorPosition, ScrollAmount, ScrollDirection, ScrollTarget, ScrollToCursorTarget,
};

pub(super) fn scroll(
    target: ScrollTarget,
    direction: ScrollDirection,
    amount: ScrollAmount,
) -> Action {
    Action::Scroll {
        target,
        direction,
        amount,
    }
}

pub(super) fn scroll_to_cursor(target: ScrollToCursorTarget, position: CursorPosition) -> Action {
    Action::ScrollToCursor { target, position }
}
