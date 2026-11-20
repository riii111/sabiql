use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ModalKind, ScrollTarget};

use super::model::{Binding, Row, hints, resolve};
use super::scroll::line_scroll;
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let action_surfaces = &[
        HintSurface::Footer,
        HintSurface::Current,
        HintSurface::Connections,
    ];
    let mut scroll = line_scroll(ScrollTarget::ConnectionError);
    scroll.description = "Scroll error";
    scroll.surfaces = &[HintSurface::Current, HintSurface::Connections];
    vec![
        Row {
            bindings: vec![Binding::action(
                Action::RetryConnection,
                &[KeyCombo::plain(Key::Char('r'))],
            )],
            short: "Retry",
            description: "Retry connection",
            visible: Some(|context, surface| {
                surface != HintSurface::Footer || context.can_retry_connection()
            }),
            surfaces: action_surfaces,
        },
        Row {
            bindings: vec![Binding::action(
                Action::ReenterConnectionSetup,
                &[KeyCombo::plain(Key::Char('e'))],
            )],
            short: "Edit",
            description: "Edit connection settings",
            visible: Some(|context, surface| {
                surface != HintSurface::Footer || !context.can_retry_connection()
            }),
            surfaces: action_surfaces,
        },
        Row {
            bindings: vec![Binding::action(
                Action::OpenModal(ModalKind::ConnectionSelector),
                &[KeyCombo::plain(Key::Char('s'))],
            )],
            short: "Switch",
            description: "Switch to another connection",
            visible: None,
            surfaces: action_surfaces,
        },
        Row {
            bindings: vec![Binding::action(
                Action::ToggleConnectionErrorDetails,
                &[KeyCombo::plain(Key::Char('d'))],
            )],
            short: "Details",
            description: "Toggle error details and requirements",
            visible: None,
            surfaces: &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Connections,
                HintSurface::Details,
            ],
        },
        Row {
            bindings: vec![Binding::action(
                Action::CopyConnectionError,
                &[KeyCombo::plain(Key::Char('y'))],
            )],
            short: "Copy",
            description: "Copy error to clipboard",
            visible: None,
            surfaces: action_surfaces,
        },
        scroll,
        Row {
            bindings: vec![Binding::action(
                Action::CloseConnectionError,
                &[KeyCombo::plain(Key::Esc)],
            )],
            short: "Close",
            description: "Close",
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Connections,
            ],
        },
    ]
});

pub fn connection_error_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn connection_error_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
