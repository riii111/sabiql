use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ModalKind, ScrollTarget};

use super::model::{Binding, Row, hints, resolve};
use super::scroll::{ScrollOptions, scroll_rows};
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![
        Row {
            bindings: vec![Binding::action(
                Action::RowDetailYank,
                &[KeyCombo::plain(Key::Char('y'))],
            )],
            short: "Copy",
            description: "Copy displayed text to clipboard",
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
        Row {
            bindings: vec![Binding::action(
                Action::RowDetailYankJson,
                &[KeyCombo::plain(Key::Char('Y'))],
            )],
            short: "Copy JSON",
            description: "Copy row as JSON to clipboard",
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
    ];
    rows.extend(scroll_rows(
        ScrollTarget::RowDetail,
        ScrollOptions {
            horizontal: true,
            ctrl_full_page: true,
            vim_jumps: true,
        },
    ));
    rows.push(Row {
        bindings: vec![Binding::action(
            Action::CloseModal(ModalKind::RowDetail),
            &[KeyCombo::plain(Key::Esc)],
        )],
        short: "Close",
        description: "Close Row Detail modal",
        dynamic_label: None,
        visible: None,
        surfaces: &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Advanced,
        ],
    });
    rows
});

pub fn row_detail_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn row_detail_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
