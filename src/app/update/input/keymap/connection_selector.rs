use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ListTarget, ModalKind};

use super::common::list_navigation;
use super::model::{Binding, Row, hints};
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut navigation = list_navigation(ListTarget::ConnectionList, true);
    navigation.short = "Nav";
    navigation.description = "Select connection";
    navigation.surfaces = &[HintSurface::Current, HintSurface::Connections];
    let button_surfaces = &[
        HintSurface::Border,
        HintSurface::Current,
        HintSurface::Connections,
    ];
    vec![
        Row::new(
            vec![Binding::action(
                Action::ConfirmConnectionSelection,
                &[KeyCombo::plain(Key::Enter)],
            )],
            "Confirm",
            "Confirm selection",
            button_surfaces,
        ),
        navigation,
        Row::new(
            vec![Binding::action(
                Action::OpenModal(ModalKind::ConnectionSetup),
                &[KeyCombo::plain(Key::Char('n'))],
            )],
            "New",
            "New connection",
            button_surfaces,
        ),
        Row {
            bindings: vec![Binding::action(
                Action::RequestEditSelectedConnection,
                &[KeyCombo::plain(Key::Char('e'))],
            )],
            short: "Edit",
            description: "Edit connection",
            references: &[],
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Border || !context.is_service_selected()
            }),
            surfaces: button_surfaces,
        },
        Row {
            bindings: vec![Binding::action(
                Action::RequestDeleteSelectedConnection,
                &[KeyCombo::plain(Key::Char('d'))],
            )],
            short: "Delete",
            description: "Delete connection",
            references: &[],
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Border || !context.is_service_selected()
            }),
            surfaces: button_surfaces,
        },
        Row::new(
            vec![Binding::action(
                Action::Escape,
                &[KeyCombo::plain(Key::Esc)],
            )],
            "Close",
            "Close selector",
            button_surfaces,
        ),
    ]
});

pub fn connection_selector_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
