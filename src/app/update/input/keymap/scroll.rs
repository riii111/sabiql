use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};

use super::HintSurface;
use super::model::{Binding, Row};

pub(super) struct ScrollOptions {
    pub horizontal: bool,
    pub ctrl_full_page: bool,
    pub vim_jumps: bool,
}

pub(super) fn scroll_rows(target: ScrollTarget, options: ScrollOptions) -> Vec<Row> {
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
    let mut rows = vec![line_scroll(target)];
    if options.horizontal {
        rows.push(Row {
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
            visible: None,
            surfaces,
        });
    }
    rows.push(Row {
        bindings: vec![
            scroll(Down, HalfPage, &[KeyCombo::ctrl(Key::Char('d'))]),
            scroll(Up, HalfPage, &[KeyCombo::ctrl(Key::Char('u'))]),
        ],
        short: "Half Page",
        description: "Scroll half page down / up",
        visible: None,
        surfaces,
    });
    let mut page_down = vec![KeyCombo::plain(Key::PageDown)];
    let mut page_up = vec![KeyCombo::plain(Key::PageUp)];
    if options.ctrl_full_page {
        page_down.push(KeyCombo::ctrl(Key::Char('f')));
        page_up.push(KeyCombo::ctrl(Key::Char('b')));
    }
    rows.push(Row {
        bindings: vec![
            scroll(Down, FullPage, &page_down),
            scroll(Up, FullPage, &page_up),
        ],
        short: "Full Page",
        description: "Scroll full page down / up",
        visible: None,
        surfaces,
    });
    let mut top = vec![KeyCombo::plain(Key::Home)];
    let mut bottom = vec![KeyCombo::plain(Key::End)];
    if options.vim_jumps {
        top.insert(0, KeyCombo::plain(Key::Char('g')));
        bottom.insert(0, KeyCombo::plain(Key::Char('G')));
    }
    rows.push(Row {
        bindings: vec![scroll(Up, ToStart, &top), scroll(Down, ToEnd, &bottom)],
        short: "Top/Btm",
        description: "Jump to top / bottom",
        visible: None,
        surfaces,
    });
    rows
}

pub(super) fn line_scroll(target: ScrollTarget) -> Row {
    let scroll = |direction, keys: &[KeyCombo]| {
        Binding::action(
            Action::Scroll {
                target,
                direction,
                amount: ScrollAmount::Line,
            },
            keys,
        )
    };
    Row {
        bindings: vec![
            scroll(
                ScrollDirection::Down,
                &[
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::plain(Key::Down),
                    KeyCombo::ctrl(Key::Char('n')),
                ],
            ),
            scroll(
                ScrollDirection::Up,
                &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::plain(Key::Up),
                    KeyCombo::ctrl(Key::Char('p')),
                ],
            ),
        ],
        short: "Scroll",
        description: "Scroll down / up",
        visible: None,
        surfaces: &[HintSurface::Current, HintSurface::Advanced],
    }
}
