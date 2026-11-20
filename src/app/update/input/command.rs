use crate::update::action::{Action, ModalKind};
use crate::update::input::keymap::parse_command;

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

pub fn parse_line_command(input: &str) -> Option<LineCommand> {
    parse_command(input)
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
            assert_eq!(parse_line_command(input), Some(LineCommand::Quit));
        }

        #[rstest]
        #[case("?")]
        #[case("help")]
        fn help_aliases(#[case] input: &str) {
            assert_eq!(parse_line_command(input), Some(LineCommand::Help));
        }

        #[test]
        fn sql_opens_sql_modal() {
            assert_eq!(
                parse_line_command("sql"),
                Some(LineCommand::Open(ModalKind::SqlModal))
            );
        }

        #[test]
        fn erd_opens_er_table_picker() {
            assert_eq!(
                parse_line_command("erd"),
                Some(LineCommand::Open(ModalKind::ErTablePicker))
            );
        }

        #[rstest]
        #[case("settings")]
        #[case("theme")]
        fn settings_aliases_open_settings(#[case] input: &str) {
            assert_eq!(
                parse_line_command(input),
                Some(LineCommand::Open(ModalKind::Settings))
            );
        }

        #[test]
        fn palette_opens_command_palette() {
            assert_eq!(
                parse_line_command("palette"),
                Some(LineCommand::Open(ModalKind::CommandPalette))
            );
        }

        #[rstest]
        #[case("w")]
        #[case("write")]
        fn write_aliases(#[case] input: &str) {
            assert_eq!(parse_line_command(input), Some(LineCommand::Write));
        }

        #[rstest]
        #[case("foo")]
        #[case("5")]
        #[case("  42  ")]
        #[case("42foo")]
        #[case("")]
        fn unknown_commands_return_none(#[case] input: &str) {
            assert_eq!(parse_line_command(input), None);
        }

        #[test]
        fn whitespace_is_trimmed() {
            assert_eq!(
                parse_line_command("  sql  "),
                Some(LineCommand::Open(ModalKind::SqlModal))
            );
        }
    }
}
