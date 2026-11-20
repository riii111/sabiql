use std::fmt::Write;

use crate::model::shared::settings::KeymapPreset;
use crate::ports::inbound::{Key, KeyCombo, Modifiers};
use crate::update::action::{Action, InputTarget};

use super::{Hint, HintSurface, KeyContext};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Trigger {
    Key(KeyCombo),
    Text,
}

#[derive(Clone)]
pub(super) enum Output {
    Action(Action),
    Text(InputTarget),
}

#[derive(Clone)]
pub(super) struct Binding {
    triggers: Vec<Trigger>,
    ide_triggers: Option<Vec<Trigger>>,
    output: Output,
}

#[derive(Clone)]
pub(super) struct Row {
    pub bindings: Vec<Binding>,
    pub short: &'static str,
    pub description: &'static str,
    pub visible: Option<fn(&KeyContext, HintSurface) -> bool>,
    pub surfaces: &'static [HintSurface],
}

impl Binding {
    pub(super) fn action(action: Action, combos: &[KeyCombo]) -> Self {
        Self {
            triggers: combos.iter().copied().map(Trigger::Key).collect(),
            ide_triggers: None,
            output: Output::Action(action),
        }
    }

    pub(super) fn text(target: InputTarget) -> Self {
        Self {
            triggers: vec![Trigger::Text],
            ide_triggers: None,
            output: Output::Text(target),
        }
    }

    fn triggers(&self, preset: KeymapPreset) -> &[Trigger] {
        match (preset, &self.ide_triggers) {
            (KeymapPreset::Ide, Some(triggers)) => triggers,
            _ => &self.triggers,
        }
    }

    fn enabled(&self, context: &KeyContext) -> bool {
        match &self.output {
            Output::Action(action) => context.allows(action),
            Output::Text(target) => context.allows(&Action::TextInput {
                target: *target,
                ch: ' ',
            }),
        }
    }
}

pub(super) fn resolve(rows: &[Row], combo: KeyCombo, context: &KeyContext) -> Option<Action> {
    rows.iter()
        .flat_map(|row| &row.bindings)
        .find_map(|binding| {
            (binding.enabled(context)
                && binding
                    .triggers(context.preset)
                    .iter()
                    .any(|trigger| matches_combo(*trigger, combo)))
            .then(|| match binding.output {
                Output::Action(ref action) => action.clone(),
                Output::Text(target) => match combo.key {
                    Key::Char(ch) => Action::TextInput { target, ch },
                    _ => unreachable!(),
                },
            })
        })
}

pub(super) fn hints(rows: &[Row], context: &KeyContext, surface: HintSurface) -> Vec<Hint> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| {
            row.surfaces.contains(&surface)
                && row.visible.is_none_or(|visible| visible(context, surface))
        })
        .filter_map(|(row_index, row)| {
            let winning: Vec<Vec<Trigger>> = row
                .bindings
                .iter()
                .enumerate()
                .filter(|(_, binding)| binding.enabled(context))
                .map(|(binding_index, binding)| {
                    binding
                        .triggers(context.preset)
                        .iter()
                        .copied()
                        .filter(|trigger| {
                            winner(rows, context, *trigger) == Some((row_index, binding_index))
                        })
                        .collect()
                })
                .filter(|triggers: &Vec<_>| !triggers.is_empty())
                .collect();
            if winning.is_empty() {
                return None;
            }
            let key = if matches!(
                surface,
                HintSurface::Current
                    | HintSurface::SearchFilter
                    | HintSurface::Advanced
                    | HintSurface::ModalBasics
            ) {
                long_name(&winning)
            } else {
                short_name(&winning)
            };
            Some(Hint {
                key,
                short: row.short,
                description: row.description,
            })
        })
        .collect()
}

fn winner(rows: &[Row], context: &KeyContext, trigger: Trigger) -> Option<(usize, usize)> {
    rows.iter().enumerate().find_map(|(row_index, row)| {
        row.bindings
            .iter()
            .enumerate()
            .find(|(_, binding)| {
                binding.enabled(context) && binding.triggers(context.preset).contains(&trigger)
            })
            .map(|(binding_index, _)| (row_index, binding_index))
    })
}

fn matches_combo(trigger: Trigger, combo: KeyCombo) -> bool {
    match trigger {
        Trigger::Key(key) => key == combo,
        Trigger::Text => {
            matches!(combo.key, Key::Char(_))
                && (!combo.modifiers.contains(Modifiers::CTRL)
                    || combo.modifiers.contains(Modifiers::ALT))
        }
    }
}

fn short_name(bindings: &[Vec<Trigger>]) -> String {
    if bindings.len() == 1 {
        bindings[0]
            .iter()
            .map(|trigger| name(*trigger, false))
            .collect::<Vec<_>>()
            .join("/")
    } else {
        bindings
            .iter()
            .map(|triggers| name(triggers[0], false))
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn long_name(bindings: &[Vec<Trigger>]) -> String {
    let max = bindings.iter().map(Vec::len).max().unwrap_or(0);
    (0..max)
        .flat_map(|i| bindings.iter().filter_map(move |triggers| triggers.get(i)))
        .map(|trigger| name(*trigger, true))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn name(trigger: Trigger, long: bool) -> String {
    let Trigger::Key(combo) = trigger else {
        return "type".to_string();
    };
    let mut result = String::new();
    if combo.modifiers.contains(Modifiers::CTRL) {
        result.push_str(if long { "Ctrl+" } else { "^" });
    }
    if combo.modifiers.contains(Modifiers::ALT) {
        result.push_str(if long { "Alt+" } else { "⌥" });
    }
    if combo.modifiers.contains(Modifiers::SHIFT) {
        result.push_str(if long { "Shift+" } else { "⇧" });
    }
    match combo.key {
        Key::Char(' ') => result.push_str("Space"),
        Key::Char(ch) if !combo.modifiers.is_empty() && ch.is_ascii_lowercase() => {
            result.push(ch.to_ascii_uppercase());
        }
        Key::Char(ch) => result.push(ch),
        Key::Enter => result.push_str("Enter"),
        Key::Esc => result.push_str("Esc"),
        Key::Tab => result.push_str("Tab"),
        Key::BackTab => result.push_str(if long { "Shift+Tab" } else { "⇧Tab" }),
        Key::Up => result.push('↑'),
        Key::Down => result.push('↓'),
        Key::Left => result.push('←'),
        Key::Right => result.push('→'),
        Key::Home => result.push_str("Home"),
        Key::End => result.push_str("End"),
        Key::Backspace => result.push_str("Backspace"),
        Key::Delete => result.push_str("Delete"),
        Key::PageUp => result.push_str(if long { "PageUp" } else { "⇞" }),
        Key::PageDown => result.push_str(if long { "PageDown" } else { "⇟" }),
        Key::F(n) => {
            let _ = write!(result, "F{n}");
        }
        Key::Null => result.push_str("Null"),
        Key::Other => result.push_str("Other"),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
    use crate::model::shared::input_mode::InputMode;
    use crate::policy::FeaturePolicy;
    use crate::update::input::keymap::cell_detail::{
        SEARCHING as CELL_SEARCH_ROWS, VIEWING as CELL_VIEW_ROWS,
    };
    use crate::update::input::keymap::confirm::ROWS as CONFIRM_ROWS;
    use crate::update::input::keymap::help::{
        EDITING as HELP_EDIT_ROWS, VIEWING as HELP_VIEW_ROWS,
    };
    use crate::update::input::keymap::pickers::{ER_ROWS, HISTORY_ROWS, PALETTE_ROWS, TABLE_ROWS};
    use crate::update::input::keymap::row_detail::ROWS as ROW_DETAIL_ROWS;
    use crate::update::input::keymap::sqlite_diagnostics::ROWS as DIAGNOSTIC_ROWS;

    #[test]
    fn text_pickers_have_unique_keys_and_keep_plain_text_unclaimed() {
        for (rows, allowed_char) in [
            (&*TABLE_ROWS, None),
            (&*HISTORY_ROWS, None),
            (&*ER_ROWS, Some(' ')),
            (&*CELL_SEARCH_ROWS, None),
            (&*HELP_EDIT_ROWS, None),
        ] {
            for preset in [KeymapPreset::Default, KeymapPreset::Ide] {
                let mut seen = Vec::new();
                for row in rows {
                    for binding in &row.bindings {
                        for trigger in binding.triggers(preset) {
                            assert!(!seen.contains(trigger));
                            seen.push(*trigger);
                            if let Trigger::Key(KeyCombo {
                                key: Key::Char(ch),
                                modifiers: Modifiers::NONE,
                            }) = trigger
                            {
                                assert_eq!(Some(*ch), allowed_char);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn visible_picker_bindings_resolve_to_the_same_actions() {
        for (rows, profile) in [
            (&*TABLE_ROWS, EngineFeatureProfile::postgres_like()),
            (&*HISTORY_ROWS, EngineFeatureProfile::postgres_like()),
            (&*PALETTE_ROWS, EngineFeatureProfile::postgres_like()),
            (&*ER_ROWS, EngineFeatureProfile::postgres_like()),
            (&*ROW_DETAIL_ROWS, EngineFeatureProfile::postgres_like()),
            (&*DIAGNOSTIC_ROWS, EngineFeatureProfile::sqlite_like()),
            (&*CELL_SEARCH_ROWS, EngineFeatureProfile::postgres_like()),
            (&*CELL_VIEW_ROWS, EngineFeatureProfile::postgres_like()),
            (&*HELP_VIEW_ROWS, EngineFeatureProfile::postgres_like()),
            (&*HELP_EDIT_ROWS, EngineFeatureProfile::postgres_like()),
            (&*CONFIRM_ROWS, EngineFeatureProfile::postgres_like()),
        ] {
            for preset in [KeymapPreset::Default, KeymapPreset::Ide] {
                let context = KeyContext::for_scope(
                    InputMode::TablePicker,
                    preset,
                    &FeaturePolicy::new(&profile),
                );
                for (row_index, row) in rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| !row.surfaces.is_empty())
                {
                    for (binding_index, binding) in row.bindings.iter().enumerate() {
                        for trigger in binding.triggers(preset) {
                            assert_eq!(
                                winner(rows, &context, *trigger),
                                Some((row_index, binding_index))
                            );
                            let combo = match trigger {
                                Trigger::Key(combo) => *combo,
                                Trigger::Text => KeyCombo::plain(Key::Char('x')),
                            };
                            let actual = resolve(rows, combo, &context).unwrap();
                            match &binding.output {
                                Output::Action(expected) => {
                                    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
                                }
                                Output::Text(target) => assert!(
                                    matches!(actual, Action::TextInput { target: actual_target, ch: 'x' } if actual_target == *target)
                                ),
                            }
                        }
                    }
                }
            }
        }
    }
}
