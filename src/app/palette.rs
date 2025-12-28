use super::action::Action;

pub struct PaletteCommand {
    pub key: &'static str,
    pub description: &'static str,
    pub action: Action,
}

pub const PALETTE_COMMANDS: &[PaletteCommand] = &[
    PaletteCommand {
        key: "q / :quit",
        description: "Quit application",
        action: Action::Quit,
    },
    PaletteCommand {
        key: "? / :help",
        description: "Show help",
        action: Action::OpenHelp,
    },
    PaletteCommand {
        key: ":sql",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
    },
    PaletteCommand {
        key: ":open-console",
        description: "Open Console (pgcli)",
        action: Action::OpenConsole,
    },
    PaletteCommand {
        key: "Ctrl+P",
        description: "Open Table Picker",
        action: Action::OpenTablePicker,
    },
    PaletteCommand {
        key: "r",
        description: "Reload metadata",
        action: Action::ReloadMetadata,
    },
];

pub fn palette_command_count() -> usize {
    PALETTE_COMMANDS.len()
}

pub fn palette_action_for_index(index: usize) -> Action {
    PALETTE_COMMANDS
        .get(index)
        .map(|cmd| cmd.action.clone())
        .unwrap_or(Action::None)
}
