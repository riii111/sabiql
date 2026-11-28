use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::Action;

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

pub fn settings_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&SELECTING, context, surface)
}
