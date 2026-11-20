use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ScrollTarget};

use super::model::{Binding, Row, hints, resolve};
use super::scroll::line_scroll;
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut scroll = line_scroll(ScrollTarget::ConfirmDialog);
    scroll.surfaces = &[HintSurface::Current, HintSurface::ModalBasics];
    vec![
        Row {
            bindings: vec![Binding::action(
                Action::ConfirmDialogConfirm,
                &[KeyCombo::plain(Key::Enter)],
            )],
            short: "Confirm",
            description: "Confirm",
            visible: Some(|context, surface| {
                surface != HintSurface::Border || !context.is_confirm_blocked()
            }),
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        },
        scroll,
        Row {
            bindings: vec![Binding::action(
                Action::ConfirmDialogCancel,
                &[KeyCombo::plain(Key::Esc)],
            )],
            short: "Cancel",
            description: "Cancel",
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        },
    ]
});

pub fn confirm_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn confirm_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
