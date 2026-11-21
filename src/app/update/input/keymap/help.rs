use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ModalKind, ScrollTarget};

use super::common::text_field;
use super::model::{Binding, Row, hints};
use super::scroll::{ScrollOptions, scroll_rows};
use super::{Hint, HintSurface, KeyContext};

pub(super) static VIEWING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = scroll_rows(
        ScrollTarget::Help,
        ScrollOptions {
            horizontal: true,
            ctrl_full_page: true,
            vim_jumps: false,
        },
    );
    for row in &mut rows {
        row.surfaces = &[
            HintSurface::Current,
            HintSurface::SearchFilter,
            HintSurface::ModalBasics,
        ];
    }
    rows.extend([
        Row::new(
            vec![Binding::action(
                Action::EnterHelpFilter,
                &[KeyCombo::plain(Key::Char('/'))],
            )],
            "Filter",
            "Edit filter",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
                HintSurface::ModalBasics,
            ],
        ),
        Row::new(
            vec![Binding::action(
                Action::CloseModal(ModalKind::Help),
                &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('?'))],
            )],
            "Close",
            "Close help",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
                HintSurface::ModalBasics,
            ],
        ),
    ]);
    rows
});

pub(super) static EDITING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![Row::new(
        vec![Binding::action(
            Action::ExitHelpFilter,
            &[KeyCombo::plain(Key::Esc)],
        )],
        "Browse",
        "Return to help browsing",
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::SearchFilter,
        ],
    )];
    rows.extend(text_field(InputTarget::HelpFilter, false));
    rows.push(Row::new(
        vec![Binding::text(InputTarget::HelpFilter)],
        "Filter",
        "Filter help",
        &[HintSurface::Current, HintSurface::SearchFilter],
    ));
    rows
});

fn rows(context: &KeyContext) -> &'static [Row] {
    if context.is_help_editing() {
        &EDITING
    } else {
        &VIEWING
    }
}

pub fn help_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows(context), context, surface)
}
