use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ModalKind, ScrollTarget};

use super::common::text_field;
use super::model::{Binding, Row, hints, resolve};
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
        Row {
            bindings: vec![Binding::action(
                Action::EnterHelpFilter,
                &[KeyCombo::plain(Key::Char('/'))],
            )],
            short: "Filter",
            description: "Edit filter",
            references: &[],
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
                HintSurface::ModalBasics,
            ],
        },
        Row {
            bindings: vec![Binding::action(
                Action::CloseModal(ModalKind::Help),
                &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('?'))],
            )],
            short: "Close",
            description: "Close help",
            references: &[],
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
                HintSurface::ModalBasics,
            ],
        },
    ]);
    rows
});

pub(super) static EDITING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![Row {
        bindings: vec![Binding::action(
            Action::ExitHelpFilter,
            &[KeyCombo::plain(Key::Esc)],
        )],
        short: "Browse",
        description: "Return to help browsing",
        references: &[],
        dynamic_label: None,
        visible: None,
        surfaces: &[HintSurface::Border, HintSurface::Current],
    }];
    rows.extend(text_field(InputTarget::HelpFilter, false));
    rows.push(Row {
        bindings: vec![Binding::text(InputTarget::HelpFilter)],
        short: "Filter",
        description: "Filter help",
        references: &[],
        dynamic_label: None,
        visible: None,
        surfaces: &[HintSurface::Current],
    });
    rows
});

fn rows(context: &KeyContext) -> &'static [Row] {
    if context.is_help_editing() {
        &EDITING
    } else {
        &VIEWING
    }
}

pub fn help_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(rows(context), combo, context).unwrap_or(Action::None)
}

pub fn help_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows(context), context, surface)
}
