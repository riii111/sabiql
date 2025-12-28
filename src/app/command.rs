#![allow(dead_code)]

use super::action::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    Help,
    Sql,
    OpenConsole,
    Unknown(String),
}

/// Parse a command string into a Command enum
pub fn parse_command(input: &str) -> Command {
    match input.trim() {
        "q" | "quit" => Command::Quit,
        "?" | "help" => Command::Help,
        "sql" => Command::Sql,
        "open-console" | "console" => Command::OpenConsole,
        other => Command::Unknown(other.to_string()),
    }
}

/// Convert a Command into an Action
pub fn command_to_action(cmd: Command) -> Action {
    match cmd {
        Command::Quit => Action::Quit,
        Command::Help => Action::OpenHelp,
        Command::Sql => Action::OpenSqlModal,
        Command::OpenConsole => Action::None, // Will be implemented in PR5
        Command::Unknown(_) => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_command {
        use super::*;

        #[test]
        fn q_returns_quit() {
            let result = parse_command("q");

            assert_eq!(result, Command::Quit);
        }

        #[test]
        fn quit_returns_quit() {
            let result = parse_command("quit");

            assert_eq!(result, Command::Quit);
        }

        #[test]
        fn question_mark_returns_help() {
            let result = parse_command("?");

            assert_eq!(result, Command::Help);
        }

        #[test]
        fn help_returns_help() {
            let result = parse_command("help");

            assert_eq!(result, Command::Help);
        }

        #[test]
        fn sql_returns_sql() {
            let result = parse_command("sql");

            assert_eq!(result, Command::Sql);
        }

        #[test]
        fn open_console_returns_open_console() {
            let result = parse_command("open-console");

            assert_eq!(result, Command::OpenConsole);
        }

        #[test]
        fn console_returns_open_console() {
            let result = parse_command("console");

            assert_eq!(result, Command::OpenConsole);
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
        fn open_console_returns_none_action() {
            let result = command_to_action(Command::OpenConsole);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn unknown_returns_none_action() {
            let result = command_to_action(Command::Unknown("foo".to_string()));

            assert!(matches!(result, Action::None));
        }
    }
}
