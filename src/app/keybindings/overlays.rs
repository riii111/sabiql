use super::types::{Key, KeyCombo};
use super::{ExecBinding, KeyBinding, ModeRow};
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

pub const HELP_ROWS: &[ModeRow] = &[
    // idx 0: SCROLL
    ModeRow {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        bindings: &[
            ExecBinding {
                action: Action::HelpScrollDown,
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::HelpScrollUp,
                combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
            },
        ],
    },
    // idx 1: CLOSE
    ModeRow {
        key_short: "?/Esc",
        key: "? / Esc",
        desc_short: "Close",
        description: "Close help",
        bindings: &[ExecBinding {
            action: Action::CloseHelp,
            combos: &[KeyCombo::plain(Key::Char('?')), KeyCombo::plain(Key::Esc)],
        }],
    },
    // idx 2: QUIT
    ModeRow {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit",
        bindings: &[ExecBinding {
            action: Action::Quit,
            combos: &[KeyCombo::plain(Key::Char('q'))],
        }],
    },
];

// =============================================================================
// Table Picker
// =============================================================================

pub const TABLE_PICKER_ROWS: &[ModeRow] = &[
    // idx 0: ENTER_SELECT
    ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Select table",
        bindings: &[ExecBinding {
            action: Action::ConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    },
    // idx 1: NAVIGATE
    ModeRow {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::SelectNext,
                combos: &[KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::SelectPrevious,
                combos: &[KeyCombo::plain(Key::Up)],
            },
        ],
    },
    // idx 2: TYPE_FILTER
    ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[ExecBinding {
            action: Action::FilterBackspace,
            combos: &[KeyCombo::plain(Key::Backspace)],
        }],
    },
    // idx 3: ESC_CLOSE
    ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseTablePicker,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    },
];

// =============================================================================
// ER Table Picker
// =============================================================================

pub const ER_PICKER_ROWS: &[ModeRow] = &[
    // idx 0: ENTER_GENERATE
    ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Generate",
        description: "Generate ER diagram",
        bindings: &[ExecBinding {
            action: Action::ErConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    },
    // idx 1: SELECT
    ModeRow {
        key_short: "Space",
        key: "Space",
        desc_short: "Select",
        description: "Toggle table selection",
        bindings: &[ExecBinding {
            action: Action::ErToggleSelection,
            combos: &[KeyCombo::plain(Key::Char(' '))],
        }],
    },
    // idx 2: SELECT_ALL
    ModeRow {
        key_short: "^A",
        key: "Ctrl+A",
        desc_short: "All",
        description: "Select/deselect all tables",
        bindings: &[ExecBinding {
            action: Action::ErSelectAll,
            combos: &[KeyCombo::ctrl(Key::Char('a'))],
        }],
    },
    // idx 3: NAVIGATE
    ModeRow {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::SelectNext,
                combos: &[KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::SelectPrevious,
                combos: &[KeyCombo::plain(Key::Up)],
            },
        ],
    },
    // idx 4: TYPE_FILTER
    ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[ExecBinding {
            action: Action::ErFilterBackspace,
            combos: &[KeyCombo::plain(Key::Backspace)],
        }],
    },
    // idx 5: ESC_CLOSE
    ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseErTablePicker,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    },
];

// =============================================================================
// Command Palette
// =============================================================================

pub const COMMAND_PALETTE_ROWS: &[ModeRow] = &[
    // idx 0: ENTER_EXECUTE
    ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        bindings: &[ExecBinding {
            action: Action::ConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    },
    // idx 1: NAVIGATE_JK
    ModeRow {
        key_short: "j/k / ↑↓",
        key: "j/k / ↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::SelectNext,
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::SelectPrevious,
                combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
            },
        ],
    },
    // idx 2: ESC_CLOSE
    ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseCommandPalette,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
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
