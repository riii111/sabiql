use std::fmt::Write;

use super::command::LineCommand;
use crate::model::shared::key_sequence::Prefix;
use crate::model::shared::settings::KeymapPreset;
use crate::policy::FeaturePolicy;
use crate::ports::inbound::{Key, KeyCombo, Modifiers};
use crate::update::action::{Action, InputTarget};

use super::{Hint, HintSurface, KeyContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Trigger {
    Key(KeyCombo),
    AnyModifiers(Key),
    NoCtrlAlt(Key),
    WithAlt(Key),
    WithCtrl(Key),
    Repeat(Key),
    Sequence {
        prefix: Prefix,
        second: KeyCombo,
        shifted_start: bool,
        any_start: bool,
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
    pub dynamic_label: Option<DynamicLabel>,
    pub visible: Option<fn(&KeyContext, HintSurface) -> bool>,
    pub surfaces: Vec<HintSurface>,
}

type DynamicLabel = fn(&KeyContext) -> (&'static str, &'static str);

impl Row {
    pub(super) fn new(
        bindings: Vec<Binding>,
        short: &'static str,
        description: &'static str,
        surfaces: &'static [HintSurface],
    ) -> Self {
        Self {
            bindings,
            short,
            description,
            dynamic_label: None,
            visible: None,
            surfaces: surfaces.to_vec(),
        }
    }
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

    pub(super) fn with_alt(action: Action, key: Key) -> Self {
        Self {
            triggers: vec![Trigger::WithAlt(key)],
            ide_triggers: None,
            output: Output::Action(action),
            condition: None,
        }
    }

    pub(super) fn with_ctrl(action: Action, key: Key) -> Self {
        Self {
            triggers: vec![Trigger::WithCtrl(key)],
            ide_triggers: None,
            output: Output::Action(action),
            condition: None,
        }
    }

    pub(super) fn repeat(action: Action, key: Key) -> Self {
        Self {
            triggers: vec![Trigger::Repeat(key)],
            ide_triggers: None,
            output: Output::Action(action),
            condition: None,
        }
    }

    pub(super) fn with_no_ctrl_alt(mut self, key: Key) -> Self {
        self.triggers.push(Trigger::NoCtrlAlt(key));
        self
    }

    pub(super) fn add_any_modifiers(mut self, key: Key) -> Self {
        self.triggers.insert(1, Trigger::AnyModifiers(key));
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
                any_start: false,
            }],
            ide_triggers: None,
            output: Output::Action(action),
            condition: None,
        }
    }

    pub(super) fn with_any_sequence_start(mut self) -> Self {
        for trigger in &mut self.triggers {
            if let Trigger::Sequence { any_start, .. } = trigger {
                *any_start = true;
            }
        }
        self
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
    if let Some(pending) = context.pending_prefix
        && (context.cancel_unmatched_sql_prefix
            || sequence_bindings(rows, context).next().is_some())
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
            any_start,
            ..
        } = trigger
            && combo.key == Key::Char(prefix_char(prefix))
            && (any_start
                || combo.modifiers.is_empty()
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

pub(super) fn text_target(rows: &[Row], context: &KeyContext) -> Option<InputTarget> {
    rows.iter()
        .flat_map(|row| &row.bindings)
        .find_map(|binding| match binding.output {
            Output::Text(target) if binding.enabled(context) => Some(target),
            _ => None,
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
    indexed_hints(rows, context, surface)
        .into_iter()
        .map(|(_, hint)| hint)
        .collect()
}

pub(super) fn indexed_hints(
    rows: &[Row],
    context: &KeyContext,
    surface: HintSurface,
) -> Vec<(usize, Hint)> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| {
            row.surfaces.contains(&surface)
                && !(surface == HintSurface::Current
                    && [HintSurface::Common, HintSurface::Navigation]
                        .iter()
                        .any(|section| row.surfaces.contains(section)))
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
            Some((row_index, display_hint(row, context, surface, &winning)))
        })
        .collect()
}

pub(super) fn reference_hint(
    row: &Row,
    preset: KeymapPreset,
    policy: &FeaturePolicy,
) -> Option<Hint> {
    let mut seen = Vec::new();
    let triggers = row
        .bindings
        .iter()
        .filter(|binding| {
            use crate::policy::FeatureRequirement;
            let feature = match &binding.output {
                Output::Action(action) => action.feature_requirement(),
                Output::Command(command) => command.action().feature_requirement(),
                Output::Text(target) => Action::TextInput {
                    target: *target,
                    ch: ' ',
                }
                .feature_requirement(),
                Output::Consume => FeatureRequirement::None,
            };
            policy.is_enabled(feature)
        })
        .map(|binding| {
            binding
                .triggers(preset)
                .iter()
                .copied()
                .filter(|trigger| {
                    let label = name(*trigger, true);
                    if seen.contains(&label) {
                        return false;
                    }
                    seen.push(label);
                    true
                })
                .collect::<Vec<_>>()
        })
        .filter(|triggers| !triggers.is_empty())
        .collect::<Vec<_>>();
    (!triggers.is_empty()).then(|| Hint {
        key: long_name(&triggers),
        short: row.short,
        description: row.description,
    })
}

pub(super) fn retained_hint(row: &Row, context: &KeyContext, surface: HintSurface) -> Hint {
    let triggers: Vec<_> = row
        .bindings
        .iter()
        .map(|binding| binding.triggers(context.preset).to_vec())
        .collect();
    display_hint(row, context, surface, &triggers)
}

pub(super) fn row_action(row: &Row) -> Option<Action> {
    row.bindings
        .iter()
        .find_map(|binding| match &binding.output {
            Output::Action(action) => Some(action.clone()),
            _ => None,
        })
}

fn display_hint(
    row: &Row,
    context: &KeyContext,
    surface: HintSurface,
    triggers: &[Vec<Trigger>],
) -> Hint {
    let key = if matches!(
        surface,
        HintSurface::Current
            | HintSurface::SearchFilter
            | HintSurface::Advanced
            | HintSurface::ModalBasics
            | HintSurface::Connections
            | HintSurface::Editing
            | HintSurface::DataActions
            | HintSurface::ExplainPlaceholder
            | HintSurface::Navigation
            | HintSurface::Common
            | HintSurface::OpenSwitch
            | HintSurface::Palette
    ) {
        long_name(triggers)
    } else {
        short_name(triggers)
    };
    let (short, description) = row
        .dynamic_label
        .map_or((row.short, row.description), |label| label(context));
    Hint {
        key,
        short,
        description,
    }
}

fn winner(rows: &[Row], context: &KeyContext, trigger: Trigger) -> Option<(usize, usize)> {
    if let Some(pending) = context.pending_prefix
        && (context.cancel_unmatched_sql_prefix
            || sequence_bindings(rows, context).next().is_some())
        && !matches!(trigger, Trigger::Sequence { prefix, .. } if prefix == pending)
    {
        return None;
    }
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
                            Trigger::Repeat(key) => matches_combo(*candidate, KeyCombo::plain(key)),
                            Trigger::WithAlt(key) => matches_combo(*candidate, KeyCombo::alt(key)),
                            Trigger::WithCtrl(key) => {
                                matches_combo(*candidate, KeyCombo::ctrl(key))
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
        Trigger::NoCtrlAlt(key) | Trigger::Repeat(key) => {
            key == combo.key && !combo.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT)
        }
        Trigger::WithAlt(key) => key == combo.key && combo.modifiers.contains(Modifiers::ALT),
        Trigger::WithCtrl(key) => key == combo.key && combo.modifiers.contains(Modifiers::CTRL),
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
        Trigger::WithAlt(key) => KeyCombo::alt(key),
        Trigger::WithCtrl(key) => KeyCombo::ctrl(key),
        Trigger::Repeat(key) => {
            let name = name(Trigger::Key(KeyCombo::plain(key)), long);
            return format!("{name}{name}");
        }
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
#[path = "contract_tests.rs"]
mod contract_tests;
