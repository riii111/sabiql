use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::ports::inbound::{MouseInput, MouseKind};

pub fn translate(event: MouseEvent) -> Option<MouseInput> {
    // Many terminals send Shift+wheel for sideways scrolling when the device has no horizontal wheel.
    let sideways = event.modifiers.contains(KeyModifiers::SHIFT);
    let kind = match event.kind {
        MouseEventKind::ScrollUp if sideways => MouseKind::ScrollLeft,
        MouseEventKind::ScrollDown if sideways => MouseKind::ScrollRight,
        MouseEventKind::ScrollUp => MouseKind::ScrollUp,
        MouseEventKind::ScrollDown => MouseKind::ScrollDown,
        MouseEventKind::ScrollLeft => MouseKind::ScrollLeft,
        MouseEventKind::ScrollRight => MouseKind::ScrollRight,
        MouseEventKind::Down(MouseButton::Left) => MouseKind::LeftClick,
        MouseEventKind::Down(_)
        | MouseEventKind::Up(_)
        | MouseEventKind::Drag(_)
        | MouseEventKind::Moved => return None,
    };
    Some(MouseInput {
        kind,
        column: event.column,
        row: event.row,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn event(kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEvent {
        MouseEvent {
            kind,
            column: 7,
            row: 3,
            modifiers,
        }
    }

    #[rstest]
    #[case(MouseEventKind::ScrollUp, MouseKind::ScrollUp)]
    #[case(MouseEventKind::ScrollDown, MouseKind::ScrollDown)]
    #[case(MouseEventKind::ScrollLeft, MouseKind::ScrollLeft)]
    #[case(MouseEventKind::ScrollRight, MouseKind::ScrollRight)]
    #[case(MouseEventKind::Down(MouseButton::Left), MouseKind::LeftClick)]
    fn translates_supported_events(#[case] kind: MouseEventKind, #[case] expected: MouseKind) {
        let input = translate(event(kind, KeyModifiers::NONE));

        assert_eq!(
            input,
            Some(MouseInput {
                kind: expected,
                column: 7,
                row: 3,
            })
        );
    }

    #[rstest]
    #[case(MouseEventKind::ScrollUp, MouseKind::ScrollLeft)]
    #[case(MouseEventKind::ScrollDown, MouseKind::ScrollRight)]
    fn shift_wheel_scrolls_sideways(#[case] kind: MouseEventKind, #[case] expected: MouseKind) {
        let input = translate(event(kind, KeyModifiers::SHIFT));

        assert_eq!(input.map(|input| input.kind), Some(expected));
    }

    #[rstest]
    #[case(MouseEventKind::Moved)]
    #[case(MouseEventKind::Up(MouseButton::Left))]
    #[case(MouseEventKind::Drag(MouseButton::Left))]
    #[case(MouseEventKind::Down(MouseButton::Right))]
    fn ignores_other_events(#[case] kind: MouseEventKind) {
        assert_eq!(translate(event(kind, KeyModifiers::NONE)), None);
    }
}
