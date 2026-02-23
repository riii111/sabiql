use super::KeyBinding;
use super::types::{Key, KeyCombo};
use crate::app::action::Action;

// =============================================================================
// Overlays (common display hints)
// =============================================================================

pub const OVERLAY_KEYS: &[KeyBinding] = &[
    // idx 0: ESC_CANCEL
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Close overlay / Cancel",
        action: Action::None,
        combos: &[],
    },
    // idx 1: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close overlay",
        action: Action::None,
        combos: &[],
    },
    // idx 2: ENTER_EXECUTE
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
        combos: &[],
    },
    // idx 3: ENTER_SELECT
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Confirm selection",
        action: Action::None,
        combos: &[],
    },
    // idx 4: NAVIGATE_JK
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate items",
        action: Action::None,
        combos: &[],
    },
    // idx 5: TYPE_FILTER
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
        combos: &[],
    },
    // idx 6: ERROR_OPEN
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Error",
        description: "View error details",
        action: Action::None,
        combos: &[],
    },
];

// =============================================================================
// Help
// =============================================================================

pub const HELP_KEYS: &[KeyBinding] = &[
    // idx 0: HELP_SCROLL (display-only)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        action: Action::None,
        combos: &[],
    },
    // idx 1: HELP_CLOSE
    KeyBinding {
        key_short: "?/Esc",
        key: "? / Esc",
        desc_short: "Close",
        description: "Close help",
        action: Action::CloseHelp,
        combos: &[KeyCombo::plain(Key::Char('?')), KeyCombo::plain(Key::Esc)],
    },
    // idx 2: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit",
        action: Action::Quit,
        combos: &[KeyCombo::plain(Key::Char('q'))],
    },
];

/// Exec-only bindings resolved by `keymap::resolve()` but excluded from display.
pub const HELP_HIDDEN: &[KeyBinding] = &[
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::HelpScrollDown,
        combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::HelpScrollUp,
        combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
    },
];

// =============================================================================
// Table Picker
// =============================================================================

pub const TABLE_PICKER_KEYS: &[KeyBinding] = &[
    // idx 0: ENTER_SELECT
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Select table",
        action: Action::ConfirmSelection,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: NAVIGATE (display-only)
    KeyBinding {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        action: Action::None,
        combos: &[],
    },
    // idx 2: TYPE_FILTER (display-only)
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
        combos: &[],
    },
    // idx 3: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        action: Action::CloseTablePicker,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

pub const TABLE_PICKER_HIDDEN: &[KeyBinding] = &[
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::SelectNext,
        combos: &[KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::SelectPrevious,
        combos: &[KeyCombo::plain(Key::Up)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::FilterBackspace,
        combos: &[KeyCombo::plain(Key::Backspace)],
    },
];

// =============================================================================
// ER Table Picker
// =============================================================================

pub const ER_PICKER_KEYS: &[KeyBinding] = &[
    // idx 0: ENTER_GENERATE
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Generate",
        description: "Generate ER diagram",
        action: Action::ErConfirmSelection,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: SELECT
    KeyBinding {
        key_short: "Space",
        key: "Space",
        desc_short: "Select",
        description: "Toggle table selection",
        action: Action::ErToggleSelection,
        combos: &[KeyCombo::plain(Key::Char(' '))],
    },
    // idx 2: SELECT_ALL
    KeyBinding {
        key_short: "^A",
        key: "Ctrl+A",
        desc_short: "All",
        description: "Select/deselect all tables",
        action: Action::ErSelectAll,
        combos: &[KeyCombo::ctrl(Key::Char('a'))],
    },
    // idx 3: NAVIGATE (display-only)
    KeyBinding {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        action: Action::None,
        combos: &[],
    },
    // idx 4: TYPE_FILTER (display-only)
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
        combos: &[],
    },
    // idx 5: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        action: Action::CloseErTablePicker,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

pub const ER_PICKER_HIDDEN: &[KeyBinding] = &[
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::SelectNext,
        combos: &[KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::SelectPrevious,
        combos: &[KeyCombo::plain(Key::Up)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::ErFilterBackspace,
        combos: &[KeyCombo::plain(Key::Backspace)],
    },
];

// =============================================================================
// Command Palette
// =============================================================================

pub const COMMAND_PALETTE_KEYS: &[KeyBinding] = &[
    // idx 0: ENTER_EXECUTE (display-only)
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
        combos: &[],
    },
    // idx 1: NAVIGATE_JK (display-only)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j/k / ↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        action: Action::None,
        combos: &[],
    },
    // idx 2: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        action: Action::CloseCommandPalette,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

pub const COMMAND_PALETTE_HIDDEN: &[KeyBinding] = &[
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::ConfirmSelection,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::SelectNext,
        combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::SelectPrevious,
        combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
    },
];

// =============================================================================
// Confirm Dialog
// =============================================================================

pub const CONFIRM_DIALOG_KEYS: &[KeyBinding] = &[
    // idx 0: CONFIRM
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm",
        action: Action::ConfirmDialogConfirm,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: CANCEL
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        action: Action::ConfirmDialogCancel,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];
