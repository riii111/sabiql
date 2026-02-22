use super::action::Action;
use super::keybindings::{GLOBAL_KEYS, KeyBinding, idx};

/// Indices in GLOBAL_KEYS excluded from the Command Palette.
/// - EXIT_FOCUS: duplicate of FOCUS (same key, context-dependent label)
/// - PANE_SWITCH: Action::None — not executable
/// - INSPECTOR_TABS: Action::None — not executable
/// - PALETTE: opening the palette from inside itself makes no sense
/// - COMMAND_LINE: command-line mode is a separate entry mechanism
const EXCLUDED_GLOBAL_INDICES: &[usize] = &[
    idx::global::PALETTE,
    idx::global::COMMAND_LINE,
    idx::global::EXIT_FOCUS,
    idx::global::PANE_SWITCH,
    idx::global::INSPECTOR_TABS,
];

fn palette_entries() -> impl Iterator<Item = &'static KeyBinding> {
    GLOBAL_KEYS
        .iter()
        .enumerate()
        .filter(|(i, _)| !EXCLUDED_GLOBAL_INDICES.contains(i))
        .map(|(_, kb)| kb)
}

pub fn palette_command_count() -> usize {
    palette_entries().count()
}

pub fn palette_action_for_index(index: usize) -> Action {
    palette_entries()
        .nth(index)
        .map(|kb| kb.action.clone())
        .unwrap_or(Action::None)
}

/// Returns an iterator of palette entries for UI rendering.
pub fn palette_commands() -> impl Iterator<Item = &'static KeyBinding> {
    palette_entries()
}
