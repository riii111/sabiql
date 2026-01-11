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
        key: "s / :sql",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
    },
    PaletteCommand {
        key: "e / :erd",
        description: "Open ER Diagram",
        action: Action::ErOpenDiagram,
    },
    PaletteCommand {
        key: "c",
        description: "Open connection settings",
        action: Action::OpenConnectionSetup,
    },
    PaletteCommand {
        key: "f",
        description: "Toggle Focus mode",
        action: Action::ToggleFocus,
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
