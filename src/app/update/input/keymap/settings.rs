use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget};

use super::common::text_field_any_modifiers;
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface, KeyContext};

pub(super) static SELECTING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    vec![
        Row {
            bindings: vec![Binding::action(
                Action::SettingsApply,
                &[KeyCombo::plain(Key::Enter)],
            )],
            short: "Apply",
            description: "Apply setting",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        Row {
            bindings: vec![
                Binding::action(
                    Action::SettingsSelectNext,
                    &[
                        KeyCombo::plain(Key::Char('j')),
                        KeyCombo::plain(Key::Down),
                        KeyCombo::ctrl(Key::Char('n')),
                    ],
                ),
                Binding::action(
                    Action::SettingsSelectPrevious,
                    &[
                        KeyCombo::plain(Key::Char('k')),
                        KeyCombo::plain(Key::Up),
                        KeyCombo::ctrl(Key::Char('p')),
                    ],
                ),
            ],
            short: "Select",
            description: "Select setting",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Footer, HintSurface::Current],
        },
        Row {
            bindings: vec![Binding::action(
                Action::SettingsStartCustomBrowserEdit,
                &[KeyCombo::plain(Key::Char('i'))],
            )],
            short: "Edit",
            description: "Edit custom browser",
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Border || context.is_custom_browser_selected()
            }),
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        Row {
            bindings: vec![
                Binding::action(Action::SettingsNextSection, &[KeyCombo::plain(Key::Tab)]),
                Binding::action(
                    Action::SettingsPreviousSection,
                    &[KeyCombo::shift(Key::BackTab)],
                ),
            ],
            short: "Section",
            description: "Switch settings section",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        Row {
            bindings: vec![Binding::action(
                Action::SettingsCancel,
                &[KeyCombo::plain(Key::Esc)],
            )],
            short: "Cancel",
            description: "Cancel",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
    ]
});

pub(super) static EDITING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![
        Row {
            bindings: vec![Binding::any_modifiers(Action::SettingsApply, Key::Enter)],
            short: "Apply",
            description: "Apply setting",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        Row {
            bindings: vec![Binding::any_modifiers(
                Action::SettingsStopCustomBrowserEdit,
                Key::Esc,
            )],
            short: "Done",
            description: "Finish editing",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
    ];
    rows.extend(text_field_any_modifiers(InputTarget::SettingsErBrowser));
    rows.push(Row {
        bindings: vec![Binding::text(InputTarget::SettingsErBrowser)],
        short: "Browser",
        description: "Edit custom browser",
        dynamic_label: None,
        visible: None,
        surfaces: &[HintSurface::Border, HintSurface::Current],
    });
    rows
});

fn rows(context: &KeyContext) -> &'static [Row] {
    if context.is_settings_editing() {
        &EDITING
    } else {
        &SELECTING
    }
}

pub fn settings_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(rows(context), combo, context).unwrap_or(Action::None)
}

pub fn settings_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows(context), context, surface)
}
