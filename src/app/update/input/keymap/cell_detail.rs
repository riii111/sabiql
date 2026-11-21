use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{
    Action, DetailTarget, InputTarget, ModalKind, ScrollTarget, SearchStep,
};

use super::common::detail_search;
use super::model::{Binding, Row, hints};
use super::scroll::{ScrollOptions, scroll_rows};
use super::{Hint, HintSurface, KeyContext};

pub(super) static VIEWING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let search = |step, key| {
        Binding::action(
            Action::DetailSearch {
                target: DetailTarget::CellDetail,
                step,
            },
            &[KeyCombo::plain(Key::Char(key))],
        )
    };
    let mut rows = vec![
        Row::new(
            vec![Binding::action(
                Action::DetailYankAll(DetailTarget::CellDetail),
                &[KeyCombo::plain(Key::Char('y'))],
            )],
            "Copy",
            "Copy full cell text",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
        Row::new(
            vec![search(SearchStep::Enter, '/')],
            "Search",
            "Search cell text",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
        Row::new(
            vec![search(SearchStep::Next, 'n'), search(SearchStep::Prev, 'N')],
            "Next/Prev",
            "Jump to next / previous search result",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
    ];
    let mut scrolling = scroll_rows(
        ScrollTarget::CellDetail,
        ScrollOptions {
            horizontal: false,
            ctrl_full_page: false,
            vim_jumps: false,
        },
    );
    scrolling[0].surfaces = &[
        HintSurface::Footer,
        HintSurface::Current,
        HintSurface::Advanced,
    ];
    rows.extend(scrolling);
    rows.push(Row::new(
        vec![Binding::action(
            Action::CloseModal(ModalKind::CellDetail),
            &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
        )],
        "Close",
        "Close cell detail",
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Advanced,
        ],
    ));
    rows
});

pub(super) static SEARCHING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    detail_search(
        DetailTarget::CellDetail,
        InputTarget::CellDetailSearch,
        false,
    )
});

fn rows(context: &KeyContext) -> &'static [Row] {
    if context.is_cell_detail_searching() {
        &SEARCHING
    } else {
        &VIEWING
    }
}

pub fn cell_detail_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows(context), context, surface)
}
