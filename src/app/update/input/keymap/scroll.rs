use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};

use super::HintSurface;
use super::model::{Binding, Row};

pub(super) fn scroll_rows(target: ScrollTarget) -> Vec<Row> {
    use ScrollAmount::{FullPage, HalfPage, Line, ToEnd, ToStart};
    use ScrollDirection::{Down, Left, Right, Up};
    let scroll = |direction, amount, keys: &[KeyCombo]| {
        Binding::action(
            Action::Scroll {
                target,
                direction,
                amount,
            },
            keys,
        )
    };
    let surfaces = &[HintSurface::Current, HintSurface::Advanced];
    vec![
        Row {
            bindings: vec![
                scroll(
                    Down,
                    Line,
                    &[
                        KeyCombo::plain(Key::Char('j')),
                        KeyCombo::plain(Key::Down),
                        KeyCombo::ctrl(Key::Char('n')),
                    ],
                ),
                scroll(
                    Up,
                    Line,
                    &[
                        KeyCombo::plain(Key::Char('k')),
                        KeyCombo::plain(Key::Up),
                        KeyCombo::ctrl(Key::Char('p')),
                    ],
                ),
            ],
            short: "Scroll",
            description: "Scroll down / up",
            surfaces,
        },
        Row {
            bindings: vec![
                scroll(
                    Left,
                    Line,
                    &[KeyCombo::plain(Key::Char('h')), KeyCombo::plain(Key::Left)],
                ),
                scroll(
                    Right,
                    Line,
                    &[KeyCombo::plain(Key::Char('l')), KeyCombo::plain(Key::Right)],
                ),
            ],
            short: "H-Scroll",
            description: "Scroll left / right",
            surfaces,
        },
        Row {
            bindings: vec![
                scroll(Down, HalfPage, &[KeyCombo::ctrl(Key::Char('d'))]),
                scroll(Up, HalfPage, &[KeyCombo::ctrl(Key::Char('u'))]),
            ],
            short: "Half Page",
            description: "Scroll half page down / up",
            surfaces,
        },
        Row {
            bindings: vec![
                scroll(
                    Down,
                    FullPage,
                    &[
                        KeyCombo::plain(Key::PageDown),
                        KeyCombo::ctrl(Key::Char('f')),
                    ],
                ),
                scroll(
                    Up,
                    FullPage,
                    &[KeyCombo::plain(Key::PageUp), KeyCombo::ctrl(Key::Char('b'))],
                ),
            ],
            short: "Full Page",
            description: "Scroll full page down / up",
            surfaces,
        },
        Row {
            bindings: vec![
                scroll(
                    Up,
                    ToStart,
                    &[KeyCombo::plain(Key::Char('g')), KeyCombo::plain(Key::Home)],
                ),
                scroll(
                    Down,
                    ToEnd,
                    &[KeyCombo::plain(Key::Char('G')), KeyCombo::plain(Key::End)],
                ),
            ],
            short: "Top/Btm",
            description: "Jump to top / bottom",
            surfaces,
        },
    ]
}
