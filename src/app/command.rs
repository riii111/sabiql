use super::action::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    Help,
    Sql,
    Erd,
    Write,
    Unknown(String),
}

/// Parse a command string into a Command enum
pub fn parse_command(input: &str) -> Command {
    match input.trim() {
        "q" | "quit" => Command::Quit,
        "?" | "help" => Command::Help,
        "sql" => Command::Sql,
        "erd" => Command::Erd,
        "w" | "write" => Command::Write,
        other => Command::Unknown(other.to_string()),
    }
}

/// Convert a Command into an Action
pub fn command_to_action(cmd: Command) -> Action {
    match cmd {
        Command::Quit => Action::Quit,
        Command::Help => Action::OpenHelp,
        Command::Sql => Action::OpenSqlModal,
        Command::Erd => Action::OpenErTablePicker,
        Command::Write => Action::SubmitCellEditWrite,
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
        fn empty_string_returns_unknown() {
            let result = parse_command("");

            assert_eq!(result, Command::Unknown("".to_string()));
        }
    }

    mod command_to_action {
        use super::*;

        #[test]
        fn quit_returns_quit_action() {
            let result = command_to_action(Command::Quit);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn help_returns_open_help_action() {
            let result = command_to_action(Command::Help);

            assert!(matches!(result, Action::OpenHelp));
        }

        #[test]
        fn sql_returns_open_sql_modal_action() {
            let result = command_to_action(Command::Sql);

            assert!(matches!(result, Action::OpenSqlModal));
        }

        #[test]
        fn erd_returns_open_er_table_picker_action() {
            let result = command_to_action(Command::Erd);

            assert!(matches!(result, Action::OpenErTablePicker));
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
