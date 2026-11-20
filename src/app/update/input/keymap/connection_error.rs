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
            references: &[],
            dynamic_label: None,
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
            references: &[],
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Footer || !context.can_retry_connection()
            }),
            surfaces: action_surfaces,
        },
        Row::new(
            vec![Binding::action(
                Action::OpenModal(ModalKind::ConnectionSelector),
                &[KeyCombo::plain(Key::Char('s'))],
            )],
            "Switch",
            "Switch to another connection",
            action_surfaces,
        ),
        Row::new(
            vec![Binding::action(
                Action::ToggleConnectionErrorDetails,
                &[KeyCombo::plain(Key::Char('d'))],
            )],
            "Details",
            "Toggle error details and requirements",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Connections,
                HintSurface::Details,
            ],
        ),
        Row::new(
            vec![Binding::action(
                Action::CopyConnectionError,
                &[KeyCombo::plain(Key::Char('y'))],
            )],
            "Copy",
            "Copy error to clipboard",
            action_surfaces,
        ),
        scroll,
        Row::new(
            vec![Binding::action(
                Action::CloseConnectionError,
                &[KeyCombo::plain(Key::Esc)],
            )],
            "Close",
            "Close",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Connections,
            ],
        ),
    ]
});

pub fn connection_error_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn connection_error_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
