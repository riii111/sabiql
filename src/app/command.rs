#![allow(dead_code)]

use super::action::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    Help,
    Sql,
    OpenConsole,
    ErdExport,
    ErdExportAndOpen,
    Unknown(String),
}

/// Parse a command string into a Command enum
pub fn parse_command(input: &str) -> Command {
    match input.trim() {
        "q" | "quit" => Command::Quit,
        "?" | "help" => Command::Help,
        "sql" => Command::Sql,
        "open-console" | "console" => Command::OpenConsole,
        "erd" => Command::ErdExport,
        "erd!" => Command::ErdExportAndOpen,
        other => Command::Unknown(other.to_string()),
    }
}

/// Convert a Command into an Action
pub fn command_to_action(cmd: Command) -> Action {
    match cmd {
        Command::Quit => Action::Quit,
        Command::Help => Action::OpenHelp,
        Command::Sql => Action::OpenSqlModal,
        Command::OpenConsole => Action::OpenConsole,
        Command::ErdExport => Action::ErExportDot,
        Command::ErdExportAndOpen => Action::ErExportDotAndOpen,
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

        #[rstest]
        #[case("open-console", Command::OpenConsole)]
        #[case("console", Command::OpenConsole)]
        fn open_console_aliases(#[case] input: &str, #[case] expected: Command) {
            let result = parse_command(input);

            assert_eq!(result, expected);
        }

        #[test]
        fn erd_returns_erd_export() {
            let result = parse_command("erd");

            assert_eq!(result, Command::ErdExport);
        }

        #[test]
        fn erd_bang_returns_erd_export_and_open() {
            let result = parse_command("erd!");

            assert_eq!(result, Command::ErdExportAndOpen);
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
        fn open_console_returns_open_console_action() {
            let result = command_to_action(Command::OpenConsole);

            assert!(matches!(result, Action::OpenConsole));
        }

        #[test]
        fn erd_export_returns_er_export_dot_action() {
            let result = command_to_action(Command::ErdExport);

            assert!(matches!(result, Action::ErExportDot));
        }

        #[test]
        fn erd_export_and_open_returns_er_export_dot_and_open_action() {
            let result = command_to_action(Command::ErdExportAndOpen);

            assert!(matches!(result, Action::ErExportDotAndOpen));
        }

        #[test]
        fn unknown_returns_none_action() {
            let result = command_to_action(Command::Unknown("foo".to_string()));

            assert!(matches!(result, Action::None));
        }
    }
}
