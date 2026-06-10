use super::keybindings::{KeyBinding, global};
use crate::update::action::Action;

// Explicit include list (display order) per the keybinding contract: palette
// entries are added deliberately, not derived from GLOBAL_KEYS.
// Intentionally absent:
// - COMMAND_LINE: command-line mode is a separate entry mechanism
// - COMMAND_PALETTE: the palette itself
// - EXIT_FOCUS: duplicate of FOCUS (same key, context-dependent label)
// - PANE_SWITCH / INSPECTOR_TABS: Action::None — not executable
const PALETTE_COMMANDS: &[KeyBinding] = &[
    global::QUIT,
    global::HELP,
    global::TABLE_PICKER,
    global::SETTINGS,
    global::FOCUS,
    global::RELOAD,
    global::SQL,
    global::ER_DIAGRAM,
    global::CONNECTIONS,
    global::CSV_EXPORT,
    global::READ_ONLY,
    global::EXIT_READ_ONLY,
    global::QUERY_HISTORY,
];

pub fn palette_command_count() -> usize {
    PALETTE_COMMANDS.len()
}

pub fn palette_action_for_index(index: usize) -> Action {
    PALETTE_COMMANDS
        .get(index)
        .map_or(Action::None, |kb| kb.action.clone())
}

pub fn palette_commands() -> impl Iterator<Item = &'static KeyBinding> {
    PALETTE_COMMANDS.iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_commands_contains_no_none_actions() {
        let none_entries: Vec<_> = palette_commands()
            .filter(|kb| matches!(kb.action, Action::None))
            .collect();

        assert!(
            none_entries.is_empty(),
            "palette_commands must not contain Action::None entries: {:?}",
            none_entries.iter().map(|kb| kb.key).collect::<Vec<_>>()
        );
    }
}
