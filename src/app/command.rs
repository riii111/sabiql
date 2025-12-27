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
        Command::Sql => Action::None, // Will be implemented in PR4
        Command::OpenConsole => Action::None, // Will be implemented in PR5
        Command::Unknown(_) => Action::None,
    }
}
