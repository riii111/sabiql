use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{
    Action, DetailTarget, InputTarget, ModalKind, ScrollTarget, SearchStep,
};

use super::common::text_field;
use super::model::{Binding, Row, hints, resolve};
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
        Row {
            bindings: vec![Binding::action(
                Action::DetailYankAll(DetailTarget::CellDetail),
                &[KeyCombo::plain(Key::Char('y'))],
            )],
            short: "Copy",
            description: "Copy full cell text",
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
        Row {
            bindings: vec![search(SearchStep::Enter, '/')],
            short: "Search",
            description: "Search cell text",
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
        Row {
            bindings: vec![search(SearchStep::Next, 'n'), search(SearchStep::Prev, 'N')],
            short: "Next/Prev",
            description: "Jump to next / previous search result",
            visible: None,
            surfaces: &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
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
    rows.push(Row {
        bindings: vec![Binding::action(
            Action::CloseModal(ModalKind::CellDetail),
            &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
        )],
        short: "Close",
        description: "Close cell detail",
        visible: None,
        surfaces: &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Advanced,
        ],
    });
    rows
});

pub(super) static SEARCHING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let target = DetailTarget::CellDetail;
    let input = InputTarget::CellDetailSearch;
    let mut rows = vec![
        Row {
            bindings: vec![Binding::action(
                Action::DetailSearch {
                    target,
                    step: SearchStep::Submit,
                },
                &[KeyCombo::plain(Key::Enter)],
            )],
            short: "Confirm",
            description: "Confirm search",
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
            ],
        },
        Row {
            bindings: vec![Binding::action(
                Action::DetailSearch {
                    target,
                    step: SearchStep::Exit,
                },
                &[KeyCombo::plain(Key::Esc)],
            )],
            short: "Cancel",
            description: "Cancel search",
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
            ],
        },
    ];
    rows.extend(text_field(input, true));
    rows.push(Row {
        bindings: vec![Binding::text(input)],
        short: "Search",
        description: "Type to search",
        visible: None,
        surfaces: &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::SearchFilter,
        ],
    });
    rows
});

fn rows(context: &KeyContext) -> &'static [Row] {
    if context.is_cell_detail_searching() {
        &SEARCHING
    } else {
        &VIEWING
    }
}

pub fn cell_detail_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(rows(context), combo, context).unwrap_or(Action::None)
}

pub fn cell_detail_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows(context), context, surface)
}
