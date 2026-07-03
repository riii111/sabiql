use crate::update::action::{Action, ModalKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    Help,
    Sql,
    Erd,
    Settings,
    Theme,
    Palette,
    Write,
    RowJsonJumpToLine(usize),
    Unknown(String),
}

pub fn parse_command(input: &str) -> Command {
    let trimmed = input.trim();
    if let Ok(line) = trimmed.parse::<usize>() {
        return Command::RowJsonJumpToLine(line);
    }
    match trimmed {
        "q" | "quit" => Command::Quit,
        "?" | "help" => Command::Help,
        "sql" => Command::Sql,
        "erd" => Command::Erd,
        "settings" => Command::Settings,
        "theme" => Command::Theme,
        "palette" => Command::Palette,
        "w" | "write" => Command::Write,
        other => Command::Unknown(other.to_string()),
    }
}

pub fn command_to_action(cmd: Command) -> Action {
    match cmd {
        Command::Quit => Action::Quit,
        Command::Help => Action::ToggleModal(ModalKind::Help),
        Command::Sql => Action::OpenModal(ModalKind::SqlModal),
        Command::Erd => Action::OpenModal(ModalKind::ErTablePicker),
        Command::Settings | Command::Theme => Action::OpenModal(ModalKind::Settings),
        Command::Palette => Action::OpenModal(ModalKind::CommandPalette),
        Command::Write => Action::SubmitCellEditWrite,
        Command::RowJsonJumpToLine(line) => Action::RowJsonJumpToLine(line),
        Command::Unknown(_) => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_command {
        use super::*;
        use rstest::rstest;

        // Aliases with equivalent behavior
        #[rstest]
        #[case("q", Command::Quit)]
        #[case("quit", Command::Quit)]
        fn quit_aliases(#[case] input: &str, #[case] expected: Command) {
            let result = parse_command(input);

            assert_eq!(result, expected);
        }

        #[rstest]
        #[case("?", Command::Help)]
        #[case("help", Command::Help)]
        fn help_aliases(#[case] input: &str, #[case] expected: Command) {
            let result = parse_command(input);

            assert_eq!(result, expected);
        }

        #[test]
        fn sql_returns_sql() {
            let result = parse_command("sql");

            assert_eq!(result, Command::Sql);
        }

        #[test]
        fn erd_returns_erd() {
            let result = parse_command("erd");

            assert_eq!(result, Command::Erd);
        }

        #[test]
        fn settings_returns_settings() {
            let result = parse_command("settings");

            assert_eq!(result, Command::Settings);
        }

        #[test]
        fn theme_returns_theme() {
            let result = parse_command("theme");

            assert_eq!(result, Command::Theme);
        }

        #[test]
        fn palette_returns_palette() {
            let result = parse_command("palette");

            assert_eq!(result, Command::Palette);
        }

        #[rstest]
        #[case("w", Command::Write)]
        #[case("write", Command::Write)]
        fn write_aliases(#[case] input: &str, #[case] expected: Command) {
            let result = parse_command(input);
            assert_eq!(result, expected);
        }

        #[test]
        fn unknown_command_returns_unknown() {
            let result = parse_command("foo");

            assert_eq!(result, Command::Unknown("foo".to_string()));
        }

        #[test]
        fn whitespace_is_trimmed() {
            let result = parse_command("  sql  ");

            assert_eq!(result, Command::Sql);
        }

        #[test]
        fn numeric_string_returns_row_json_jump_to_line() {
            let result = parse_command("5");

            assert_eq!(result, Command::RowJsonJumpToLine(5));
        }

        #[test]
        fn numeric_string_with_whitespace_is_trimmed() {
            let result = parse_command("  42  ");

            assert_eq!(result, Command::RowJsonJumpToLine(42));
        }

        #[test]
        fn non_numeric_string_returns_unknown() {
            let result = parse_command("42foo");

            assert_eq!(result, Command::Unknown("42foo".to_string()));
        }

        #[test]
        fn empty_string_returns_unknown() {
            let result = parse_command("");

            assert_eq!(result, Command::Unknown(String::new()));
        }
    }

    mod command_to_action {
        use super::*;

        #[test]
        fn row_json_jump_to_line_returns_action() {
            let result = command_to_action(Command::RowJsonJumpToLine(7));

            assert!(matches!(result, Action::RowJsonJumpToLine(7)));
        }

        #[test]
        fn quit_returns_quit_action() {
            let result = command_to_action(Command::Quit);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn help_returns_open_help_action() {
            let result = command_to_action(Command::Help);

            assert!(matches!(result, Action::ToggleModal(ModalKind::Help)));
        }

        #[test]
        fn sql_returns_open_sql_modal_action() {
            let result = command_to_action(Command::Sql);

            assert!(matches!(result, Action::OpenModal(ModalKind::SqlModal)));
        }

        #[test]
        fn erd_returns_open_er_table_picker_action() {
            let result = command_to_action(Command::Erd);

            assert!(matches!(
                result,
                Action::OpenModal(ModalKind::ErTablePicker)
            ));
        }

        #[test]
        fn settings_returns_open_settings_action() {
            let result = command_to_action(Command::Settings);

            assert!(matches!(result, Action::OpenModal(ModalKind::Settings)));
        }

        #[test]
        fn theme_returns_open_settings_action() {
            let result = command_to_action(Command::Theme);

            assert!(matches!(result, Action::OpenModal(ModalKind::Settings)));
        }

        #[test]
        fn palette_returns_open_command_palette_action() {
            let result = command_to_action(Command::Palette);

            assert!(matches!(
                result,
                Action::OpenModal(ModalKind::CommandPalette)
            ));
        }

        #[test]
        fn write_returns_submit_cell_edit_write_action() {
            let result = command_to_action(Command::Write);
            assert!(matches!(result, Action::SubmitCellEditWrite));
        }

        #[test]
        fn unknown_returns_none_action() {
            let result = command_to_action(Command::Unknown("foo".to_string()));

            assert!(matches!(result, Action::None));
        }
    }
}
