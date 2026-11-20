use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ModalKind};
use crate::update::input::command::LineCommand;

use super::common::text_field_any_modifiers;
use super::model::{Binding, Row, hints, resolve, resolve_command};
use super::{Hint, HintSurface, KeyContext};

static COMMANDS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    [
        (
            LineCommand::Quit,
            "quit",
            &["q"][..],
            "Quit",
            "Quit application",
        ),
        (LineCommand::Help, "help", &["?"][..], "Help", "Show help"),
        (
            LineCommand::Open(ModalKind::SqlModal),
            "sql",
            &[][..],
            "SQL",
            "Open SQL Editor",
        ),
        (
            LineCommand::Open(ModalKind::ErTablePicker),
            "erd",
            &[][..],
            "ER Diagram",
            "Open ER Diagram",
        ),
        (
            LineCommand::Open(ModalKind::Settings),
            "settings",
            &["theme"][..],
            "Settings",
            "Open Settings",
        ),
        (
            LineCommand::Open(ModalKind::CommandPalette),
            "palette",
            &[][..],
            "Palette",
            "Open Command Palette",
        ),
        (
            LineCommand::Write,
            "w",
            &["write"][..],
            "Write",
            "Preview and confirm UPDATE",
        ),
    ]
    .into_iter()
    .map(|(command, name, aliases, short, description)| Row {
        bindings: vec![Binding::command(command, name, aliases)],
        short,
        description,
        dynamic_label: None,
        visible: if command == LineCommand::Write {
            Some(|context, surface| {
                surface != HintSurface::FooterAction
                    || context.browse_draft()
                    || context.browse_staged_delete()
            })
        } else {
            None
        },
        surfaces: if command == LineCommand::Write {
            &[
                HintSurface::Current,
                HintSurface::Write,
                HintSurface::FooterAction,
            ]
        } else {
            &[HintSurface::Current]
        },
    })
    .collect()
});

pub(super) static EDITING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![
        Row {
            bindings: vec![Binding::action(
                Action::CommandLineSubmit,
                &[KeyCombo::plain(Key::Enter)],
            )],
            short: "Submit",
            description: "Submit command",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Footer, HintSurface::Current],
        },
        Row {
            bindings: vec![Binding::action(
                Action::ExitCommandLine,
                &[KeyCombo::plain(Key::Esc)],
            )],
            short: "Exit",
            description: "Exit command line",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Footer, HintSurface::Current],
        },
    ];
    rows.extend(text_field_any_modifiers(InputTarget::CommandLine));
    rows.push(Row {
        bindings: vec![Binding::text(InputTarget::CommandLine)],
        short: "Type",
        description: "Enter command",
        dynamic_label: None,
        visible: None,
        surfaces: &[],
    });
    rows
});

pub fn command_line_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&EDITING, combo, context).unwrap_or(Action::None)
}

pub fn command_line_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    let mut result = if surface == HintSurface::Current {
        hints(&COMMANDS, context, surface)
    } else {
        Vec::new()
    };
    result.extend(hints(&EDITING, context, surface));
    result
}

pub(super) fn write_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    if matches!(
        surface,
        HintSurface::Footer | HintSurface::Current | HintSurface::Editing
    ) {
        hints(&COMMANDS, context, HintSurface::Write)
    } else {
        Vec::new()
    }
}

pub(super) fn browse_write_hints(context: &KeyContext) -> Vec<Hint> {
    hints(&COMMANDS, context, HintSurface::FooterAction)
}

pub fn parse_command(input: &str) -> Option<LineCommand> {
    resolve_command(&COMMANDS, input)
}
