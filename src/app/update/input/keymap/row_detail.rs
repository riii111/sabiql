use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ModalKind, ScrollTarget};

use super::model::{Binding, Row, hints};
use super::scroll::{ScrollOptions, scroll_rows};
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![
        Row::new(
            vec![Binding::action(
                Action::RowDetailYank,
                &[KeyCombo::plain(Key::Char('y'))],
            )],
            "Copy",
            "Copy displayed text to clipboard",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
        Row::new(
            vec![Binding::action(
                Action::RowDetailYankJson,
                &[KeyCombo::plain(Key::Char('Y'))],
            )],
            "Copy JSON",
            "Copy row as JSON to clipboard",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
    ];
    rows.extend(scroll_rows(
        ScrollTarget::RowDetail,
        ScrollOptions {
            horizontal: true,
            ctrl_full_page: true,
            vim_jumps: true,
        },
    ));
    rows.push(Row::new(
        vec![Binding::action(
            Action::CloseModal(ModalKind::RowDetail),
            &[KeyCombo::plain(Key::Esc)],
        )],
        "Close",
        "Close Row Detail modal",
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Advanced,
        ],
    ));
    rows
});

pub fn row_detail_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
