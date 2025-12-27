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
        description: "Open SQL Modal (PR4)",
        action: Action::None,
    },
    PaletteCommand {
        key: ":open-console",
        description: "Open Console (PR5)",
        action: Action::None,
    },
    PaletteCommand {
        key: "Ctrl+P",
        description: "Open Table Picker",
        action: Action::OpenTablePicker,
    },
    PaletteCommand {
        key: "f",
        description: "Toggle Focus mode",
        action: Action::ToggleFocus,
    },
    PaletteCommand {
        key: "r",
        description: "Reload metadata (PR3)",
        action: Action::None,
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
