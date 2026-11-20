use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ModalKind};

use super::common::text_field_any_modifiers;
use super::model::{Binding, Row, hints, resolve_command};
use super::{Hint, HintSurface, KeyContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCommand {
    Quit,
    Help,
    Open(ModalKind),
    Write,
}

impl LineCommand {
    pub(crate) fn action(self) -> Action {
        match self {
            Self::Quit => Action::Quit,
            Self::Help => Action::ToggleHelp,
            Self::Open(kind) => Action::OpenModal(kind),
            Self::Write => Action::SubmitCellEditWrite,
        }
    }
}

static WRITE: LazyLock<Row> = LazyLock::new(|| {
    command_row(
        LineCommand::Write,
        "w",
        &["write"],
        "Write",
        "Preview and confirm changes",
    )
});

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
    ]
    .into_iter()
    .map(|(command, name, aliases, short, description)| {
        command_row(command, name, aliases, short, description)
    })
    .chain(std::iter::once(WRITE.clone()))
    .collect()
});

fn command_row(
    command: LineCommand,
    name: &'static str,
    aliases: &'static [&'static str],
    short: &'static str,
    description: &'static str,
) -> Row {
    Row {
        bindings: vec![Binding::command(command, name, aliases)],
        short,
        description,
        references: match command {
            LineCommand::Write => &[HintSurface::Editing],
            LineCommand::Open(_) => &[HintSurface::OpenSwitch],
            LineCommand::Quit | LineCommand::Help => &[HintSurface::Common],
        },
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
            &[HintSurface::Current, HintSurface::FooterAction]
        } else {
            &[HintSurface::Current]
        },
    }
}

pub(super) static EDITING: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = COMMANDS.clone();
    rows.extend([
        Row::new(
            vec![Binding::action(
                Action::CommandLineSubmit,
                &[KeyCombo::plain(Key::Enter)],
            )],
            "Submit",
            "Submit command",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        ),
        Row::new(
            vec![Binding::action(
                Action::ExitCommandLine,
                &[KeyCombo::plain(Key::Esc)],
            )],
            "Exit",
            "Exit command line",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::ModalBasics,
            ],
        ),
    ]);
    rows.extend(text_field_any_modifiers(InputTarget::CommandLine));
    rows.push(Row::new(
        vec![Binding::text(InputTarget::CommandLine)],
        "Type",
        "Enter command",
        &[HintSurface::ModalBasics],
    ));
    rows
});

pub fn command_line_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&EDITING, context, surface)
}

pub(super) fn cell_edit_write_row() -> Row {
    let mut row = WRITE.clone();
    row.surfaces = &[
        HintSurface::Footer,
        HintSurface::Current,
        HintSurface::Editing,
    ];
    row.references = &[];
    row
}

pub(super) fn browse_write_hints(context: &KeyContext) -> Vec<Hint> {
    hints(&COMMANDS, context, HintSurface::FooterAction)
}

pub fn parse_command(input: &str) -> Option<LineCommand> {
    resolve_command(&COMMANDS, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_line_command {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("q")]
        #[case("quit")]
        fn quit_aliases(#[case] input: &str) {
            assert_eq!(parse_command(input), Some(LineCommand::Quit));
        }

        #[rstest]
        #[case("?")]
        #[case("help")]
        fn help_aliases(#[case] input: &str) {
            assert_eq!(parse_command(input), Some(LineCommand::Help));
        }

        #[test]
        fn sql_opens_sql_modal() {
            assert_eq!(
                parse_command("sql"),
                Some(LineCommand::Open(ModalKind::SqlModal))
            );
        }

        #[test]
        fn erd_opens_er_table_picker() {
            assert_eq!(
                parse_command("erd"),
                Some(LineCommand::Open(ModalKind::ErTablePicker))
            );
        }

        #[rstest]
        #[case("settings")]
        #[case("theme")]
        fn settings_aliases_open_settings(#[case] input: &str) {
            assert_eq!(
                parse_command(input),
                Some(LineCommand::Open(ModalKind::Settings))
            );
        }

        #[test]
        fn palette_opens_command_palette() {
            assert_eq!(
                parse_command("palette"),
                Some(LineCommand::Open(ModalKind::CommandPalette))
            );
        }

        #[rstest]
        #[case("w")]
        #[case("write")]
        fn write_aliases(#[case] input: &str) {
            assert_eq!(parse_command(input), Some(LineCommand::Write));
        }

        #[rstest]
        #[case("foo")]
        #[case("5")]
        #[case("  42  ")]
        #[case("42foo")]
        #[case("")]
        fn unknown_commands_return_none(#[case] input: &str) {
            assert_eq!(parse_command(input), None);
        }

        #[test]
        fn whitespace_is_trimmed() {
            assert_eq!(
                parse_command("  sql  "),
                Some(LineCommand::Open(ModalKind::SqlModal))
            );
        }
    }
}
