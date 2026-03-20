use super::types::{Key, KeyCombo};
use super::{ExecBinding, KeyBinding, ModeRow};
use crate::app::action::{
    Action, InputTarget, ListMotion, ListTarget, ScrollAmount, ScrollDirection, ScrollTarget,
    SelectMotion,
};

// =============================================================================
// Overlays (common display hints)
// =============================================================================

pub const OVERLAY_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Close overlay / Cancel",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close overlay",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Confirm selection",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate items",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
        combos: &[],
    },
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
    ModeRow {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
            },
        ],
    },
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
];

// =============================================================================
// Table Picker
// =============================================================================

pub const TABLE_PICKER_ROWS: &[ModeRow] = &[
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
    ModeRow {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::Select(SelectMotion::Next),
                combos: &[KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::Select(SelectMotion::Previous),
                combos: &[KeyCombo::plain(Key::Up)],
            },
        ],
    },
    ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[ExecBinding {
            action: Action::TextBackspace {
                target: InputTarget::Filter,
            },
            combos: &[KeyCombo::plain(Key::Backspace)],
        }],
    },
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
    ModeRow {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::Select(SelectMotion::Next),
                combos: &[KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::Select(SelectMotion::Previous),
                combos: &[KeyCombo::plain(Key::Up)],
            },
        ],
    },
    ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[ExecBinding {
            action: Action::TextBackspace {
                target: InputTarget::ErFilter,
            },
            combos: &[KeyCombo::plain(Key::Backspace)],
        }],
    },
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
// Query History Picker
// =============================================================================

pub const QUERY_HISTORY_PICKER_ROWS: &[ModeRow] = &[
    ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Select query",
        bindings: &[ExecBinding {
            action: Action::QueryHistoryConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    },
    ModeRow {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Previous,
                },
                combos: &[KeyCombo::plain(Key::Up)],
            },
        ],
    },
    ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[ExecBinding {
            action: Action::TextBackspace {
                target: InputTarget::QueryHistoryFilter,
            },
            combos: &[KeyCombo::plain(Key::Backspace)],
        }],
    },
    ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseQueryHistoryPicker,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    },
];

// =============================================================================
// Command Palette
// =============================================================================

pub const COMMAND_PALETTE_ROWS: &[ModeRow] = &[
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
    ModeRow {
        key_short: "j/k / ↑↓",
        key: "j/k / ↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::Select(SelectMotion::Next),
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::Select(SelectMotion::Previous),
                combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
            },
        ],
    },
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
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm",
        action: Action::ConfirmDialogConfirm,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        action: Action::ConfirmDialogCancel,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];
