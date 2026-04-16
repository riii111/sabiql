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
        key_short: "^N/^P/j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
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
        key_short: "^N/^P/j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::plain(Key::Down),
                    KeyCombo::ctrl(Key::Char('n')),
                ],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::plain(Key::Up),
                    KeyCombo::ctrl(Key::Char('p')),
                ],
            },
        ],
    },
    ModeRow {
        key_short: "g/G/Home/End",
        key: "g / Home / G / End",
        desc_short: "Top/Btm",
        description: "Jump to top / bottom",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart,
                },
                combos: &[KeyCombo::plain(Key::Char('g')), KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                combos: &[KeyCombo::plain(Key::Char('G')), KeyCombo::plain(Key::End)],
            },
        ],
    },
    ModeRow {
        key_short: "^D/^U",
        key: "Ctrl+D / Ctrl+U",
        desc_short: "Half Page",
        description: "Scroll half page down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('d'))],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('u'))],
            },
        ],
    },
    ModeRow {
        key_short: "^F/^B/PgDn/Up",
        key: "Ctrl+F / Ctrl+B / PageDown / PageUp",
        desc_short: "Full Page",
        description: "Scroll full page down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[
                    KeyCombo::ctrl(Key::Char('f')),
                    KeyCombo::plain(Key::PageDown),
                ],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('b')), KeyCombo::plain(Key::PageUp)],
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
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Previous,
                },
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
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
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Nav",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Previous,
                },
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
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
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Previous,
                },
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
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
        key_short: "^N/^P/j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Next,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::plain(Key::Down),
                    KeyCombo::ctrl(Key::Char('n')),
                ],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Previous,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::plain(Key::Up),
                    KeyCombo::ctrl(Key::Char('p')),
                ],
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
        key_short: "^N/j/↓",
        key: "Ctrl+N / j / ↓",
        desc_short: "Down",
        description: "Scroll down",
        action: Action::Scroll {
            target: ScrollTarget::ConfirmDialog,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        },
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Down),
            KeyCombo::ctrl(Key::Char('n')),
        ],
    },
    KeyBinding {
        key_short: "^P/k/↑",
        key: "Ctrl+P / k / ↑",
        desc_short: "Up",
        description: "Scroll up",
        action: Action::Scroll {
            target: ScrollTarget::ConfirmDialog,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        },
        combos: &[
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::ctrl(Key::Char('p')),
        ],
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

pub const JSONB_DETAIL_ROWS: &[ModeRow] = &[
    ModeRow {
        key_short: "y",
        key: "y",
        desc_short: "Copy",
        description: "Copy full JSON",
        bindings: &[ExecBinding {
            action: Action::JsonbYankAll,
            combos: &[KeyCombo::plain(Key::Char('y'))],
        }],
    },
    ModeRow {
        key_short: "i",
        key: "i / A",
        desc_short: "Insert",
        description: "Enter Insert mode / append at line end",
        bindings: &[
            ExecBinding {
                action: Action::JsonbEnterEdit,
                combos: &[KeyCombo::plain(Key::Char('i'))],
            },
            ExecBinding {
                action: Action::JsonbAppendInsert,
                combos: &[KeyCombo::plain(Key::Char('A'))],
            },
        ],
    },
    ModeRow {
        key_short: "/",
        key: "/",
        desc_short: "Search",
        description: "Search JSON text",
        bindings: &[ExecBinding {
            action: Action::JsonbEnterSearch,
            combos: &[KeyCombo::plain(Key::Char('/'))],
        }],
    },
    ModeRow {
        key_short: "n/N",
        key: "n / N",
        desc_short: "Next/Prev",
        description: "Jump to next / previous search result",
        bindings: &[
            ExecBinding {
                action: Action::JsonbSearchNext,
                combos: &[KeyCombo::plain(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::JsonbSearchPrev,
                combos: &[KeyCombo::plain(Key::Char('N'))],
            },
        ],
    },
    ModeRow {
        key_short: "hjkl/↑↓←→",
        key: "h / j / k / l / ↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Char('h')), KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Char('l')), KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::ctrl(Key::Char('n')),
                    KeyCombo::plain(Key::Down),
                ],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Up,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::ctrl(Key::Char('p')),
                    KeyCombo::plain(Key::Up),
                ],
            },
        ],
    },
    ModeRow {
        key_short: "0$wb",
        key: "0 / $ / w / b / Home / End",
        desc_short: "Jump",
        description: "Move by word or line boundary",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineStart,
                },
                combos: &[KeyCombo::plain(Key::Char('0'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineEnd,
                },
                combos: &[KeyCombo::plain(Key::Char('$'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::WordForward,
                },
                combos: &[KeyCombo::plain(Key::Char('w'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::WordBackward,
                },
                combos: &[KeyCombo::plain(Key::Char('b'))],
            },
        ],
    },
    ModeRow {
        key_short: "ggGHML",
        key: "gg / G / H / M / L",
        desc_short: "View",
        description: "Jump by buffer or viewport",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LastLine,
                },
                combos: &[KeyCombo::plain(Key::Char('G'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::ViewportTop,
                },
                combos: &[KeyCombo::plain(Key::Char('H'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::ViewportMiddle,
                },
                combos: &[KeyCombo::plain(Key::Char('M'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::ViewportBottom,
                },
                combos: &[KeyCombo::plain(Key::Char('L'))],
            },
        ],
    },
    ModeRow {
        key_short: "Esc",
        key: "Esc / q",
        desc_short: "Close",
        description: "Close JSONB detail",
        bindings: &[ExecBinding {
            action: Action::CloseJsonbDetail,
            combos: &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
        }],
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

pub const JSONB_EDIT_ROWS: &[ModeRow] = &[
    ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Return to Normal mode",
        bindings: &[ExecBinding {
            action: Action::JsonbExitEdit,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    },
    ModeRow {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Up,
                },
                combos: &[KeyCombo::plain(Key::Up)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                },
                combos: &[KeyCombo::plain(Key::Down)],
            },
        ],
    },
    ModeRow {
        key_short: "Home/End",
        key: "Home / End",
        desc_short: "Line",
        description: "Line start/end",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    },
];
