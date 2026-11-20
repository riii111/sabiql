use std::fmt::Write;

use crate::model::shared::key_sequence::Prefix;
use crate::model::shared::settings::KeymapPreset;
use crate::ports::inbound::{Key, KeyCombo, Modifiers};
use crate::update::action::{Action, InputTarget};
use crate::update::input::command::LineCommand;

use super::{Hint, HintSurface, KeyContext};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Trigger {
    Key(KeyCombo),
    AnyModifiers(Key),
    NoCtrlAlt(Key),
    Sequence {
        prefix: Prefix,
        second: KeyCombo,
        shifted_start: bool,
    },
    Command {
        name: &'static str,
        aliases: &'static [&'static str],
    },
    Text,
    AnyText,
}

#[derive(Clone)]
pub(super) enum Output {
    Action(Action),
    Text(InputTarget),
    Command(LineCommand),
    Consume,
}

#[derive(Clone)]
pub(super) struct Binding {
    triggers: Vec<Trigger>,
    ide_triggers: Option<Vec<Trigger>>,
    output: Output,
    condition: Option<fn(&KeyContext) -> bool>,
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
            condition: None,
        }
    }

    pub(super) fn text(target: InputTarget) -> Self {
        Self {
            triggers: vec![Trigger::Text],
            ide_triggers: None,
            output: Output::Text(target),
            condition: None,
        }
    }

    pub(super) fn text_any_modifiers(target: InputTarget) -> Self {
        Self {
            triggers: vec![Trigger::AnyText],
            ide_triggers: None,
            output: Output::Text(target),
            condition: None,
        }
    }

    pub(super) fn consume(mut self) -> Self {
        self.output = Output::Consume;
        self
    }

    pub(super) fn any_modifiers(action: Action, key: Key) -> Self {
        Self {
            triggers: vec![Trigger::AnyModifiers(key)],
            ide_triggers: None,
            output: Output::Action(action),
            condition: None,
        }
    }

    pub(super) fn no_ctrl_alt(action: Action, key: Key) -> Self {
        Self {
            triggers: vec![Trigger::NoCtrlAlt(key)],
            ide_triggers: None,
            output: Output::Action(action),
            condition: None,
        }
    }

    pub(super) fn with_no_ctrl_alt(mut self, key: Key) -> Self {
        self.triggers.push(Trigger::NoCtrlAlt(key));
        self
    }

    pub(super) fn with_any_modifiers(mut self, key: Key) -> Self {
        self.triggers = vec![Trigger::AnyModifiers(key)];
        self
    }

    pub(super) fn command(
        command: LineCommand,
        name: &'static str,
        aliases: &'static [&'static str],
    ) -> Self {
        Self {
            triggers: vec![Trigger::Command { name, aliases }],
            ide_triggers: None,
            output: Output::Command(command),
            condition: None,
        }
    }

    pub(super) fn sequence(
        action: Action,
        prefix: Prefix,
        second: KeyCombo,
        shifted_start: bool,
    ) -> Self {
        Self {
            triggers: vec![Trigger::Sequence {
                prefix,
                second,
                shifted_start,
            }],
            ide_triggers: None,
            output: Output::Action(action),
            condition: None,
        }
    }

    pub(super) fn when(mut self, condition: fn(&KeyContext) -> bool) -> Self {
        self.condition = Some(condition);
        self
    }

    pub(super) fn ide(mut self, combos: &[KeyCombo]) -> Self {
        self.ide_triggers = Some(combos.iter().copied().map(Trigger::Key).collect());
        self
    }

    fn triggers(&self, preset: KeymapPreset) -> &[Trigger] {
        match (preset, &self.ide_triggers) {
            (KeymapPreset::Ide, Some(triggers)) => triggers,
            _ => &self.triggers,
        }
    }

    fn enabled(&self, context: &KeyContext) -> bool {
        self.condition.is_none_or(|condition| condition(context))
            && match &self.output {
                Output::Action(action) => context.allows(action),
                Output::Text(target) => context.allows(&Action::TextInput {
                    target: *target,
                    ch: ' ',
                }),
                Output::Command(command) => context.allows(&command.action()),
                Output::Consume => true,
            }
    }
}

pub(super) fn resolve(rows: &[Row], combo: KeyCombo, context: &KeyContext) -> Option<Action> {
    if let Some(pending) = context.pending_prefix()
        && sequence_bindings(rows, context).next().is_some()
    {
        for (binding, trigger) in sequence_bindings(rows, context) {
            if let Trigger::Sequence { prefix, second, .. } = trigger
                && prefix == pending
                && second == combo
                && let Output::Action(action) = &binding.output
            {
                return Some(action.clone());
            }
        }
        return Some(Action::CancelKeySequence);
    }
    for (_, trigger) in sequence_bindings(rows, context) {
        if let Trigger::Sequence {
            prefix,
            shifted_start,
            ..
        } = trigger
            && combo.key == Key::Char(prefix_char(prefix))
            && (combo.modifiers.is_empty()
                || (shifted_start && combo.modifiers == Modifiers::SHIFT))
        {
            let action = Action::BeginKeySequence(prefix);
            return context.allows(&action).then_some(action);
        }
    }
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
                Output::Command(_) => unreachable!(),
                Output::Consume => Action::None,
            })
        })
}

fn sequence_bindings<'a>(
    rows: &'a [Row],
    context: &'a KeyContext,
) -> impl Iterator<Item = (&'a Binding, Trigger)> {
    rows.iter()
        .flat_map(|row| &row.bindings)
        .filter(|binding| binding.enabled(context))
        .flat_map(|binding| {
            binding
                .triggers(context.preset)
                .iter()
                .copied()
                .map(move |trigger| (binding, trigger))
        })
        .filter(|(_, trigger)| matches!(trigger, Trigger::Sequence { .. }))
}

pub(super) fn resolve_command(rows: &[Row], input: &str) -> Option<LineCommand> {
    let input = input.trim();
    rows.iter()
        .flat_map(|row| &row.bindings)
        .find_map(|binding| {
            binding
                .triggers
                .iter()
                .find_map(|trigger| match (trigger, &binding.output) {
                    (Trigger::Command { name, aliases }, Output::Command(command))
                        if *name == input || aliases.contains(&input) =>
                    {
                        Some(*command)
                    }
                    _ => None,
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
                    | HintSurface::Connections
                    | HintSurface::Editing
                    | HintSurface::DataActions
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
                binding.enabled(context)
                    && binding
                        .triggers(context.preset)
                        .iter()
                        .any(|candidate| match trigger {
                            Trigger::Key(combo) => matches_combo(*candidate, combo),
                            Trigger::AnyModifiers(key) | Trigger::NoCtrlAlt(key) => {
                                matches_combo(*candidate, KeyCombo::plain(key))
                            }
                            Trigger::Text | Trigger::AnyText => {
                                matches_combo(*candidate, KeyCombo::plain(Key::Char('x')))
                            }
                            Trigger::Command { .. } | Trigger::Sequence { .. } => {
                                *candidate == trigger
                            }
                        })
            })
            .map(|(binding_index, _)| (row_index, binding_index))
    })
}

fn matches_combo(trigger: Trigger, combo: KeyCombo) -> bool {
    match trigger {
        Trigger::Key(mut key) => {
            let mut combo = combo;
            if key.key == Key::BackTab && combo.key == Key::BackTab {
                key.modifiers.set(Modifiers::SHIFT, false);
                combo.modifiers.set(Modifiers::SHIFT, false);
            }
            key == combo
        }
        Trigger::AnyModifiers(key) => key == combo.key,
        Trigger::NoCtrlAlt(key) => {
            key == combo.key && !combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT)
        }
        Trigger::Sequence { .. } | Trigger::Command { .. } => false,
        Trigger::Text => {
            matches!(combo.key, Key::Char(_))
                && (!combo.modifiers.contains(Modifiers::CTRL)
                    || combo.modifiers.contains(Modifiers::ALT))
        }
        Trigger::AnyText => matches!(combo.key, Key::Char(_)),
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
    let combo = match trigger {
        Trigger::Key(combo) => combo,
        Trigger::AnyModifiers(key) | Trigger::NoCtrlAlt(key) => KeyCombo::plain(key),
        Trigger::Sequence { prefix, second, .. } => {
            return format!(
                "{}{}",
                prefix_char(prefix),
                name(Trigger::Key(second), long)
            );
        }
        Trigger::Command { name, .. } => return format!(":{name}"),
        Trigger::Text | Trigger::AnyText => return "type".to_string(),
    };
    let mut result = String::new();
    if combo.modifiers.contains(Modifiers::CTRL) {
        result.push_str(if long { "Ctrl+" } else { "^" });
    }
    if combo.modifiers.contains(Modifiers::ALT) {
        result.push_str(if long { "Alt+" } else { "⌥" });
    }
    if combo.modifiers.contains(Modifiers::SHIFT) && combo.key != Key::BackTab {
        result.push_str(if long { "Shift+" } else { "⇧" });
    }
    match combo.key {
        Key::Char(' ') => result.push_str("Space"),
        Key::Char(ch)
            if combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT)
                && ch.is_ascii_lowercase() =>
        {
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

fn prefix_char(prefix: Prefix) -> char {
    match prefix {
        Prefix::G => 'g',
        Prefix::Z => 'z',
    }
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
    use crate::update::input::keymap::cell_edit::ROWS as CELL_EDIT_ROWS;
    use crate::update::input::keymap::command::EDITING as COMMAND_EDIT_ROWS;
    use crate::update::input::keymap::confirm::ROWS as CONFIRM_ROWS;
    use crate::update::input::keymap::connection_error::ROWS as CONNECTION_ERROR_ROWS;
    use crate::update::input::keymap::connection_selector::ROWS as CONNECTION_SELECTOR_ROWS;
    use crate::update::input::keymap::connection_setup::active_rows as setup_rows;
    use crate::update::input::keymap::help::{
        EDITING as HELP_EDIT_ROWS, VIEWING as HELP_VIEW_ROWS,
    };
    use crate::update::input::keymap::json_edit::ROWS as JSON_EDIT_ROWS;
    use crate::update::input::keymap::json_search::ROWS as JSON_SEARCH_ROWS;
    use crate::update::input::keymap::json_view::ROWS as JSON_VIEW_ROWS;
    use crate::update::input::keymap::pickers::{ER_ROWS, HISTORY_ROWS, PALETTE_ROWS, TABLE_ROWS};
    use crate::update::input::keymap::row_detail::ROWS as ROW_DETAIL_ROWS;
    use crate::update::input::keymap::settings::{
        EDITING as SETTINGS_EDIT_ROWS, SELECTING as SETTINGS_ROWS,
    };
    use crate::update::input::keymap::sql_confirmation::active_rows as confirmation_rows;
    use crate::update::input::keymap::sqlite_diagnostics::ROWS as DIAGNOSTIC_ROWS;

    #[test]
    fn text_pickers_have_unique_keys_and_keep_plain_text_unclaimed() {
        for (rows, allowed_char) in [
            (&*TABLE_ROWS, None),
            (&*HISTORY_ROWS, None),
            (&*ER_ROWS, Some(' ')),
            (&*CELL_SEARCH_ROWS, None),
            (&*JSON_SEARCH_ROWS, None),
            (&*SETTINGS_EDIT_ROWS, None),
            (&*COMMAND_EDIT_ROWS, None),
            (&*CELL_EDIT_ROWS, Some(':')),
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
            (&*JSON_SEARCH_ROWS, EngineFeatureProfile::postgres_like()),
            (&*JSON_EDIT_ROWS, EngineFeatureProfile::postgres_like()),
            (&*JSON_VIEW_ROWS, EngineFeatureProfile::postgres_like()),
            (&*CELL_VIEW_ROWS, EngineFeatureProfile::postgres_like()),
            (&*HELP_VIEW_ROWS, EngineFeatureProfile::postgres_like()),
            (&*HELP_EDIT_ROWS, EngineFeatureProfile::postgres_like()),
            (&*CONFIRM_ROWS, EngineFeatureProfile::postgres_like()),
            (&*SETTINGS_ROWS, EngineFeatureProfile::postgres_like()),
            (&*SETTINGS_EDIT_ROWS, EngineFeatureProfile::postgres_like()),
            (&*COMMAND_EDIT_ROWS, EngineFeatureProfile::postgres_like()),
            (&*CELL_EDIT_ROWS, EngineFeatureProfile::postgres_like()),
            (
                &*CONNECTION_ERROR_ROWS,
                EngineFeatureProfile::postgres_like(),
            ),
            (
                &*CONNECTION_SELECTOR_ROWS,
                EngineFeatureProfile::postgres_like(),
            ),
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
                        if !binding.enabled(&context) {
                            continue;
                        }
                        for trigger in binding.triggers(preset) {
                            if winner(rows, &context, *trigger) != Some((row_index, binding_index))
                            {
                                continue;
                            }
                            let combo = match trigger {
                                Trigger::Key(combo) => *combo,
                                Trigger::AnyModifiers(key) | Trigger::NoCtrlAlt(key) => {
                                    KeyCombo::plain(*key)
                                }
                                Trigger::Sequence { prefix, second, .. } => {
                                    let Output::Action(expected) = &binding.output else {
                                        unreachable!()
                                    };
                                    let pending = context.clone().with_prefix(Some(*prefix));
                                    assert_eq!(
                                        format!("{:?}", resolve(rows, *second, &pending)),
                                        format!("{:?}", Some(expected))
                                    );
                                    continue;
                                }
                                Trigger::Command { .. } => continue,
                                Trigger::Text | Trigger::AnyText => KeyCombo::plain(Key::Char('x')),
                            };
                            let actual = resolve(rows, combo, &context).unwrap();
                            match &binding.output {
                                Output::Action(expected) => {
                                    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
                                }
                                Output::Text(target) => assert!(
                                    matches!(actual, Action::TextInput { target: actual_target, ch: 'x' } if actual_target == *target)
                                ),
                                Output::Command(_) => unreachable!(),
                                Output::Consume => assert!(matches!(actual, Action::None)),
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn visible_setup_bindings_resolve_through_event_entry() {
        use crate::model::app_state::AppState;
        use crate::ports::inbound::InputEvent;
        use crate::update::input::handle_event;

        for preset in [KeymapPreset::Default, KeymapPreset::Ide] {
            for (text_field, dropdown_open) in [(false, false), (false, true), (true, false)] {
                let mut state = AppState::new("test".to_string());
                state.modal.set_mode(InputMode::ConnectionSetup);
                state.settings.load_keymap_preset(preset);
                if text_field {
                    state.connection_setup.focus_next_field();
                }
                if dropdown_open {
                    state.connection_setup.toggle_focused_dropdown();
                }
                let context = KeyContext::from_state(&state);
                let rows = setup_rows(&context);
                for (row_index, row) in rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.surfaces.contains(&HintSurface::Border))
                {
                    for (binding_index, binding) in row
                        .bindings
                        .iter()
                        .enumerate()
                        .filter(|(_, binding)| binding.enabled(&context))
                    {
                        for trigger in binding.triggers(preset) {
                            if winner(rows, &context, *trigger) != Some((row_index, binding_index))
                            {
                                continue;
                            }
                            let combo = match trigger {
                                Trigger::Key(combo) => *combo,
                                Trigger::AnyModifiers(key) | Trigger::NoCtrlAlt(key) => {
                                    KeyCombo::plain(*key)
                                }
                                Trigger::Text
                                | Trigger::AnyText
                                | Trigger::Command { .. }
                                | Trigger::Sequence { .. } => {
                                    continue;
                                }
                            };
                            let Output::Action(expected) = &binding.output else {
                                unreachable!()
                            };
                            assert_eq!(
                                format!("{:?}", handle_event(InputEvent::Key(combo), &state)),
                                format!("{expected:?}")
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn visible_confirmation_bindings_resolve_through_event_entry() {
        use crate::model::app_state::AppState;
        use crate::policy::write::sql_risk::AcknowledgeReason;
        use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
        use crate::ports::inbound::InputEvent;
        use crate::update::{input::handle_event, test_fixtures};

        for preset in [KeymapPreset::Default, KeymapPreset::Ide] {
            for kind in 0..4 {
                let mut state = AppState::new("test".to_string());
                test_fixtures::activate_postgres_connection(
                    &mut state,
                    "postgres://localhost/test",
                );
                state.modal.set_mode(InputMode::SqlModal);
                state.settings.load_keymap_preset(preset);
                match kind {
                    0 => {
                        state.sql_modal.begin_confirming_high(
                            AdhocRiskDecision {
                                risk_level: RiskLevel::High,
                                label: "DROP",
                            },
                            "users".to_string(),
                        );
                        state
                            .sql_modal
                            .confirming_high_input_mut()
                            .unwrap()
                            .set_content("users".to_string());
                    }
                    1 => state
                        .sql_modal
                        .begin_confirming_risk(AcknowledgeReason::UnknownRisk, "risk".to_string()),
                    2 => {
                        state.sql_modal.begin_confirming_analyze_high(
                            "DELETE".to_string(),
                            "users".to_string(),
                        );
                        state
                            .sql_modal
                            .confirming_analyze_high_input_mut()
                            .unwrap()
                            .set_content("users".to_string());
                    }
                    _ => state.sql_modal.begin_confirming_analyze_risk(
                        "DELETE".to_string(),
                        AcknowledgeReason::AnalyzeExecution,
                    ),
                }
                let context = KeyContext::from_state(&state);
                let rows = confirmation_rows(&context).unwrap();
                for (row_index, row) in rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.surfaces.contains(&HintSurface::Border))
                {
                    assert!(
                        row.visible
                            .is_none_or(|visible| visible(&context, HintSurface::Border))
                    );
                    for (binding_index, binding) in row
                        .bindings
                        .iter()
                        .enumerate()
                        .filter(|(_, binding)| binding.enabled(&context))
                    {
                        for trigger in binding.triggers(preset) {
                            if winner(rows, &context, *trigger) != Some((row_index, binding_index))
                            {
                                continue;
                            }
                            let combo = match trigger {
                                Trigger::Key(combo) => *combo,
                                Trigger::AnyModifiers(key) | Trigger::NoCtrlAlt(key) => {
                                    KeyCombo::plain(*key)
                                }
                                _ => continue,
                            };
                            let Output::Action(expected) = &binding.output else {
                                unreachable!()
                            };
                            assert_eq!(
                                format!("{:?}", handle_event(InputEvent::Key(combo), &state)),
                                format!("{expected:?}")
                            );
                        }
                    }
                }
            }
        }
    }
}
