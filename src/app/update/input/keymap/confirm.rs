use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ScrollTarget};

use super::model::{Binding, Row, hints};
use super::scroll::line_scroll;
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut scroll = line_scroll(ScrollTarget::ConfirmDialog);
    scroll.surfaces = [HintSurface::Current, HintSurface::ModalBasics].to_vec();
    vec![
        Row {
            bindings: vec![Binding::action(
                Action::ConfirmDialogConfirm,
                &[KeyCombo::plain(Key::Enter)],
            )],
            short: "Confirm",
            description: "Confirm",

            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Border || !context.is_confirm_blocked
            }),
            surfaces: [
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ]
            .to_vec(),
        },
        scroll,
        Row::new(
            vec![Binding::action(
                Action::ConfirmDialogCancel,
                &[KeyCombo::plain(Key::Esc)],
            )],
            "Cancel",
            "Cancel",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        ),
    ]
});

pub fn confirm_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
