pub(super) mod browse;
pub(super) mod json;
pub(super) mod sql;

use crate::update::action::{
    Action, CursorMove, CursorPosition, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget,
    ScrollToCursorTarget,
};
use crate::update::input::vim::types::VimNavigation;

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

pub(super) fn text_cursor(navigation: VimNavigation, target: InputTarget) -> Option<Action> {
    let direction = match navigation {
        VimNavigation::MoveLeft => CursorMove::Left,
        VimNavigation::MoveRight => CursorMove::Right,
        VimNavigation::MoveUp => CursorMove::Up,
        VimNavigation::MoveDown => CursorMove::Down,
        VimNavigation::MoveToFirst => CursorMove::FirstLine,
        VimNavigation::MoveToLast => CursorMove::LastLine,
        VimNavigation::MoveLineStart => CursorMove::LineStart,
        VimNavigation::MoveLineEnd => CursorMove::LineEnd,
        VimNavigation::MoveWordForward => CursorMove::WordForward,
        VimNavigation::MoveWordBackward => CursorMove::WordBackward,
        VimNavigation::ViewportTop => CursorMove::ViewportTop,
        VimNavigation::ViewportMiddle => CursorMove::ViewportMiddle,
        VimNavigation::ViewportBottom => CursorMove::ViewportBottom,
        _ => return None,
    };

    Some(Action::TextMoveCursor { target, direction })
}
