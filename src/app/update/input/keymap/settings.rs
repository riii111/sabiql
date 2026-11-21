use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget};

use super::common::text_field_any_modifiers;
use super::model::{Binding, Row, hints};
use super::{Hint, HintSurface, KeyContext};

pub(super) static SELECTING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    vec![
        Row::new(
            vec![Binding::action(
                Action::SettingsApply,
                &[KeyCombo::plain(Key::Enter)],
            )],
            "Apply",
            "Apply setting",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        ),
        Row::new(
            vec![
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
            "Select",
            "Select setting",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        ),
        Row {
            bindings: vec![Binding::action(
                Action::SettingsStartCustomBrowserEdit,
                &[KeyCombo::plain(Key::Char('i'))],
            )],
            short: "Edit",
            description: "Edit custom browser",
            references: &[HintSurface::Editing],
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Border || context.is_custom_browser_selected()
            }),
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        Row::new(
            vec![
                Binding::action(Action::SettingsNextSection, &[KeyCombo::plain(Key::Tab)]),
                Binding::action(
                    Action::SettingsPreviousSection,
                    &[KeyCombo::shift(Key::BackTab)],
                ),
            ],
            "Section",
            "Switch settings section",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        ),
        Row::new(
            vec![Binding::action(
                Action::SettingsCancel,
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

pub(super) static EDITING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![
        Row::new(
            vec![Binding::any_modifiers(Action::SettingsApply, Key::Enter)],
            "Apply",
            "Apply setting",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        ),
        Row::new(
            vec![Binding::any_modifiers(
                Action::SettingsStopCustomBrowserEdit,
                Key::Esc,
            )],
            "Done",
            "Finish editing",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        ),
    ];
    rows.extend(text_field_any_modifiers(InputTarget::SettingsErBrowser));
    rows.push(Row::new(
        vec![Binding::text(InputTarget::SettingsErBrowser)],
        "Browser",
        "Edit custom browser",
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    ));
    rows
});

fn rows(context: &KeyContext) -> &'static [Row] {
    if context.is_settings_editing() {
        &EDITING
    } else {
        &SELECTING
    }
}

pub fn settings_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows(context), context, surface)
}
