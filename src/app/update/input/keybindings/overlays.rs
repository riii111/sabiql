use super::types::{Key, KeyCombo};
use super::{ExecBinding, KeyBinding, ModeRow};
use crate::app::update::action::{
    Action, CursorMove, InputTarget, ListMotion, ListTarget, ScrollAmount, ScrollDirection,
    ScrollTarget,
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
                action: Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::TablePicker,
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
        bindings: &[
            ExecBinding {
                action: Action::TextBackspace {
                    target: InputTarget::Filter,
                },
                combos: &[KeyCombo::plain(Key::Backspace)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
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
                action: Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::ErTablePicker,
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
        bindings: &[
            ExecBinding {
                action: Action::TextBackspace {
                    target: InputTarget::ErFilter,
                },
                combos: &[KeyCombo::plain(Key::Backspace)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
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
        bindings: &[
            ExecBinding {
                action: Action::TextBackspace {
                    target: InputTarget::QueryHistoryFilter,
                },
                combos: &[KeyCombo::plain(Key::Backspace)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
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
                action: Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Previous,
                },
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

// =============================================================================
// JSONB Detail (Viewing)
// =============================================================================

pub const JSONB_DETAIL_KEYS: &[KeyBinding] = &[
    // Actions
    KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Copy",
        description: "Copy full JSON",
        action: Action::JsonbYankAll,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    },
    KeyBinding {
        key_short: "i",
        key: "i",
        desc_short: "Edit",
        description: "Enter JSON editor",
        action: Action::JsonbEnterEdit,
        combos: &[KeyCombo::plain(Key::Char('i'))],
    },
    KeyBinding {
        key_short: "/",
        key: "/",
        desc_short: "Search",
        description: "Search within JSON",
        action: Action::JsonbEnterSearch,
        combos: &[KeyCombo::plain(Key::Char('/'))],
    },
    // Navigation
    KeyBinding {
        key_short: "j/↓",
        key: "j / ↓",
        desc_short: "Down",
        description: "Move cursor down",
        action: Action::JsonbCursorDown,
        combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "k/↑",
        key: "k / ↑",
        desc_short: "Up",
        description: "Move cursor up",
        action: Action::JsonbCursorUp,
        combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
    },
    KeyBinding {
        key_short: "h/l",
        key: "h / l / ← / →",
        desc_short: "Fold",
        description: "Collapse / Expand",
        action: Action::JsonbToggleFold,
        combos: &[
            KeyCombo::plain(Key::Char('h')),
            KeyCombo::plain(Key::Char('l')),
            KeyCombo::plain(Key::Left),
            KeyCombo::plain(Key::Right),
        ],
    },
    KeyBinding {
        key_short: "Enter",
        key: "Enter / Space",
        desc_short: "Toggle",
        description: "Toggle fold",
        action: Action::JsonbToggleFold,
        combos: &[KeyCombo::plain(Key::Enter), KeyCombo::plain(Key::Char(' '))],
    },
    KeyBinding {
        key_short: "H",
        key: "H",
        desc_short: "FoldAll",
        description: "Fold all nodes",
        action: Action::JsonbFoldAll,
        combos: &[KeyCombo::plain(Key::Char('H'))],
    },
    KeyBinding {
        key_short: "L",
        key: "L",
        desc_short: "UnfoldAll",
        description: "Unfold all nodes",
        action: Action::JsonbUnfoldAll,
        combos: &[KeyCombo::plain(Key::Char('L'))],
    },
    KeyBinding {
        key_short: "g",
        key: "g",
        desc_short: "Top",
        description: "Scroll to top",
        action: Action::JsonbScrollToTop,
        combos: &[KeyCombo::plain(Key::Char('g'))],
    },
    KeyBinding {
        key_short: "G",
        key: "G",
        desc_short: "Bottom",
        description: "Scroll to bottom",
        action: Action::JsonbScrollToEnd,
        combos: &[KeyCombo::plain(Key::Char('G'))],
    },
    KeyBinding {
        key_short: "n",
        key: "n / N",
        desc_short: "Next/Prev",
        description: "Next / Previous match",
        action: Action::JsonbSearchNext,
        combos: &[KeyCombo::plain(Key::Char('n'))],
    },
    KeyBinding {
        key_short: "N",
        key: "N",
        desc_short: "Prev",
        description: "Previous match",
        action: Action::JsonbSearchPrev,
        combos: &[KeyCombo::plain(Key::Char('N'))],
    },
    // Close
    KeyBinding {
        key_short: "Esc",
        key: "Esc / q",
        desc_short: "Close",
        description: "Close JSONB detail",
        action: Action::CloseJsonbDetail,
        combos: &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
    },
];

// =============================================================================
// JSONB Search (active search input)
// =============================================================================

pub const JSONB_SEARCH_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Search",
        description: "Type to search",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm search",
        action: Action::JsonbSearchSubmit,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel search",
        action: Action::JsonbExitSearch,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

// =============================================================================
// JSONB Edit
// =============================================================================

pub const JSONB_EDIT_KEYS: &[KeyBinding] = &[KeyBinding {
    key_short: "Esc",
    key: "Esc",
    desc_short: "Back",
    description: "Return to viewer / apply changes",
    action: Action::JsonbExitEdit,
    combos: &[KeyCombo::plain(Key::Esc)],
}];
