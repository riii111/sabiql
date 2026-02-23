//! Centralized keybinding definitions.
//! Single source of truth for key/description used by Footer, Help, and Palette.

use super::action::Action;

// =============================================================================
// App-layer key types (crossterm-free)
// =============================================================================

/// Application-level key code, independent of the terminal backend.
/// Only includes keys that sabiql actually uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    PageUp,
    PageDown,
    F(u8),
    Null,
    Other,
}

/// Modifier flags for a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
    };
    pub const ALT: Self = Self {
        ctrl: false,
        alt: true,
        shift: false,
    };
    pub const SHIFT: Self = Self {
        ctrl: false,
        alt: false,
        shift: true,
    };
}

/// A key + modifier combination, used as the app-layer abstraction for input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyCombo {
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::NONE,
        }
    }
    pub const fn ctrl(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::CTRL,
        }
    }
    pub const fn alt(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::ALT,
        }
    }
    pub const fn shift(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::SHIFT,
        }
    }
}

// =============================================================================
// KeyBinding
// =============================================================================

#[derive(Clone)]
pub struct KeyBinding {
    /// Short key for Footer (e.g., "^P", "j/k")
    pub key_short: &'static str,
    /// Display key for Help/Palette (e.g., "Ctrl+P", "j / ↓")
    pub key: &'static str,
    /// Short description for Footer (e.g., "Quit", "Scroll")
    pub desc_short: &'static str,
    /// Full description for Help/Palette (e.g., "Quit application")
    pub description: &'static str,
    /// The action triggered by this key.
    ///
    /// `Action::None` means **display-only**: the entry is shown in Footer/Help/Palette
    /// as a hint but is not matched by `handler.rs`. This is used for multi-key
    /// combined display (e.g., `"j/k / ↑↓"`) or navigation descriptions where the
    /// actual matching is handled directly in handler match arms.
    pub action: Action,
    /// The key combinations that trigger this binding.
    /// For display-only entries (`Action::None`), these represent the keys
    /// shown in the hint text. For executable entries, `keymap::resolve()`
    /// matches against these combos.
    pub combos: &'static [KeyCombo],
}

impl KeyBinding {
    /// Returns (key_short, desc_short) tuple for Footer display
    pub const fn as_hint(&self) -> (&'static str, &'static str) {
        (self.key_short, self.desc_short)
    }
}

// =============================================================================
// Index Constants for Footer Lookup
// =============================================================================

pub mod idx {
    pub mod global {
        pub const QUIT: usize = 0;
        pub const HELP: usize = 1;
        pub const TABLE_PICKER: usize = 2;
        pub const PALETTE: usize = 3;
        pub const COMMAND_LINE: usize = 4;
        pub const FOCUS: usize = 5;
        pub const EXIT_FOCUS: usize = 6;
        pub const PANE_SWITCH: usize = 7;
        pub const INSPECTOR_TABS: usize = 8;
        pub const RELOAD: usize = 9;
        pub const SQL: usize = 10;
        pub const ER_DIAGRAM: usize = 11;
        pub const CONNECTIONS: usize = 12;
    }

    /// Indexes for FOOTER_NAV_KEYS
    pub mod footer_nav {
        pub const SCROLL: usize = 0;
        pub const SCROLL_SHORT: usize = 1;
        pub const TOP_BOTTOM: usize = 2;
        pub const H_SCROLL: usize = 3;
        pub const PAGE_NAV: usize = 4;
    }

    pub mod sql_modal {
        pub const RUN: usize = 0;
        pub const ESC_CLOSE: usize = 1;
        pub const MOVE: usize = 2;
        pub const HOME_END: usize = 3;
        pub const TAB: usize = 4;
        pub const COMPLETION_TRIGGER: usize = 5;
        pub const CLEAR: usize = 6;
    }

    pub mod overlay {
        pub const ESC_CANCEL: usize = 0;
        pub const ESC_CLOSE: usize = 1;
        pub const ENTER_EXECUTE: usize = 2;
        pub const ENTER_SELECT: usize = 3;
        pub const NAVIGATE_JK: usize = 4;
        pub const TYPE_FILTER: usize = 5;
        pub const ERROR_OPEN: usize = 6;
    }

    pub mod conn_setup {
        pub const TAB_NAV: usize = 0;
        pub const TAB_NEXT: usize = 1;
        pub const TAB_PREV: usize = 2;
        pub const SAVE: usize = 3;
        pub const ESC_CANCEL: usize = 4;
        pub const ENTER_DROPDOWN: usize = 5;
        pub const DROPDOWN_NAV: usize = 6;
    }

    pub mod conn_error {
        pub const EDIT: usize = 0;
        pub const SWITCH: usize = 1;
        pub const DETAILS: usize = 2;
        pub const COPY: usize = 3;
        pub const SCROLL: usize = 4;
        pub const ESC_CLOSE: usize = 5;
        pub const QUIT: usize = 6;
    }

    pub mod confirm {
        pub const YES: usize = 0;
        pub const NO: usize = 1;
    }

    pub mod table_picker {
        pub const ENTER_SELECT: usize = 0;
        pub const NAVIGATE: usize = 1;
        pub const TYPE_FILTER: usize = 2;
        pub const ESC_CLOSE: usize = 3;
    }

    pub mod er_picker {
        pub const ENTER_GENERATE: usize = 0;
        pub const SELECT: usize = 1;
        pub const SELECT_ALL: usize = 2;
        pub const NAVIGATE: usize = 3;
        pub const TYPE_FILTER: usize = 4;
        pub const ESC_CLOSE: usize = 5;
    }

    pub mod cmd_palette {
        pub const ENTER_EXECUTE: usize = 0;
        pub const NAVIGATE_JK: usize = 1;
        pub const ESC_CLOSE: usize = 2;
    }

    pub mod help {
        pub const SCROLL: usize = 0;
        pub const CLOSE: usize = 1;
        pub const QUIT: usize = 2;
    }

    pub mod result_active {
        pub const ENTER_DEEPEN: usize = 0;
        pub const YANK: usize = 1;
        pub const STAGE_DELETE: usize = 2;
        pub const UNSTAGE_DELETE: usize = 3;
        pub const CELL_NAV: usize = 4;
        pub const ROW_NAV: usize = 5;
        pub const TOP_BOTTOM: usize = 6;
        pub const ESC_BACK: usize = 7;
        pub const EDIT: usize = 8;
        pub const DRAFT_DISCARD: usize = 9;
    }

    pub mod cell_edit {
        pub const WRITE: usize = 0;
        pub const TYPE: usize = 1;
        pub const COMMAND: usize = 2;
        pub const ESC_CANCEL: usize = 3;
    }

    pub mod connections_mode {
        pub const CONNECT: usize = 0;
        pub const NEW: usize = 1;
        pub const EDIT: usize = 2;
        pub const DELETE: usize = 3;
        pub const NAVIGATE: usize = 4;
        pub const HELP: usize = 5;
        pub const TABLES: usize = 6;
        pub const BACK: usize = 7;
        pub const QUIT: usize = 8;
    }

    pub mod connection_selector {
        pub const CONFIRM: usize = 0;
        pub const SELECT: usize = 1;
        pub const NEW: usize = 2;
        pub const EDIT: usize = 3;
        pub const DELETE: usize = 4;
        pub const QUIT: usize = 5;
    }
}

// =============================================================================
// Global Keys (Normal mode)
// =============================================================================

pub const GLOBAL_KEYS: &[KeyBinding] = &[
    // idx 0: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[KeyCombo::plain(Key::Char('q'))],
    },
    // idx 1: HELP
    KeyBinding {
        key_short: "?",
        key: "?",
        desc_short: "Help",
        description: "Toggle help",
        action: Action::OpenHelp,
        combos: &[KeyCombo::plain(Key::Char('?'))],
    },
    // idx 2: TABLE_PICKER
    KeyBinding {
        key_short: "^P",
        key: "Ctrl+P",
        desc_short: "Tables",
        description: "Open Table Picker",
        action: Action::OpenTablePicker,
        combos: &[KeyCombo::ctrl(Key::Char('p'))],
    },
    // idx 3: PALETTE
    KeyBinding {
        key_short: "^K",
        key: "Ctrl+K",
        desc_short: "Palette",
        description: "Open Command Palette",
        action: Action::OpenCommandPalette,
        combos: &[KeyCombo::ctrl(Key::Char('k'))],
    },
    // idx 4: COMMAND_LINE
    KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Enter command line",
        action: Action::EnterCommandLine,
        combos: &[KeyCombo::plain(Key::Char(':'))],
    },
    // idx 5: FOCUS
    KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Focus",
        description: "Toggle Focus mode",
        action: Action::ToggleFocus,
        combos: &[KeyCombo::plain(Key::Char('f'))],
    },
    // idx 6: EXIT_FOCUS (same key, different display)
    KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Exit Focus",
        description: "Exit Focus mode",
        action: Action::ToggleFocus,
        combos: &[KeyCombo::plain(Key::Char('f'))],
    },
    // idx 7: PANE_SWITCH
    KeyBinding {
        key_short: "1/2/3",
        key: "1/2/3",
        desc_short: "Pane",
        description: "Switch pane focus",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('1')),
            KeyCombo::plain(Key::Char('2')),
            KeyCombo::plain(Key::Char('3')),
        ],
    },
    // idx 8: INSPECTOR_TABS
    KeyBinding {
        key_short: "Tab/⇧Tab",
        key: "Tab/⇧Tab",
        desc_short: "InsTabs",
        description: "Inspector prev/next tab",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Tab), KeyCombo::plain(Key::BackTab)],
    },
    // idx 9: RELOAD
    KeyBinding {
        key_short: "r",
        key: "r",
        desc_short: "Reload",
        description: "Reload metadata",
        action: Action::ReloadMetadata,
        combos: &[KeyCombo::plain(Key::Char('r'))],
    },
    // idx 10: SQL
    KeyBinding {
        key_short: "s",
        key: "s",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
        combos: &[KeyCombo::plain(Key::Char('s'))],
    },
    // idx 11: ER_DIAGRAM
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::OpenErTablePicker,
        combos: &[KeyCombo::plain(Key::Char('e'))],
    },
    // idx 12: CONNECTIONS
    KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Connections",
        description: "Toggle Connections mode",
        action: Action::ToggleExplorerMode,
        combos: &[KeyCombo::plain(Key::Char('c'))],
    },
];

/// Navigation keys for Help overlay (individual key display)
pub const NAVIGATION_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "j",
        key: "j / ↓",
        desc_short: "Down",
        description: "Move down / scroll",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "k",
        key: "k / ↑",
        desc_short: "Up",
        description: "Move up / scroll",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
    },
    KeyBinding {
        key_short: "g",
        key: "g / Home",
        desc_short: "Top",
        description: "First item / top",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Char('g')), KeyCombo::plain(Key::Home)],
    },
    KeyBinding {
        key_short: "G",
        key: "G / End",
        desc_short: "Bottom",
        description: "Last item / bottom",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Char('G')), KeyCombo::plain(Key::End)],
    },
    KeyBinding {
        key_short: "^D/^U",
        key: "Ctrl+D / Ctrl+U",
        desc_short: "Half Page",
        description: "Scroll half page down/up",
        action: Action::None,
        combos: &[
            KeyCombo::ctrl(Key::Char('d')),
            KeyCombo::ctrl(Key::Char('u')),
        ],
    },
    KeyBinding {
        key_short: "^F/^B",
        key: "Ctrl+F/B / PgDn/Up",
        desc_short: "Full Page",
        description: "Scroll full page down/up",
        action: Action::None,
        combos: &[
            KeyCombo::ctrl(Key::Char('f')),
            KeyCombo::ctrl(Key::Char('b')),
            KeyCombo::plain(Key::PageDown),
            KeyCombo::plain(Key::PageUp),
        ],
    },
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('h')),
            KeyCombo::plain(Key::Char('l')),
            KeyCombo::plain(Key::Left),
            KeyCombo::plain(Key::Right),
        ],
    },
    KeyBinding {
        key_short: "]",
        key: "]",
        desc_short: "Next Page",
        description: "Next page (Preview)",
        action: Action::ResultNextPage,
        combos: &[KeyCombo::plain(Key::Char(']'))],
    },
    KeyBinding {
        key_short: "[",
        key: "[",
        desc_short: "Prev Page",
        description: "Previous page (Preview)",
        action: Action::ResultPrevPage,
        combos: &[KeyCombo::plain(Key::Char('['))],
    },
];

/// Navigation keys for Footer (combined key display)
pub const FOOTER_NAV_KEYS: &[KeyBinding] = &[
    // idx 0: SCROLL
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
        ],
    },
    // idx 1: SCROLL_SHORT (same as SCROLL for now)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
        ],
    },
    // idx 2: TOP_BOTTOM
    KeyBinding {
        key_short: "g/G",
        key: "g / G",
        desc_short: "Top/Bottom",
        description: "First/Last item",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('g')),
            KeyCombo::plain(Key::Char('G')),
        ],
    },
    // idx 3: H_SCROLL
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l / ← / →",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('h')),
            KeyCombo::plain(Key::Char('l')),
            KeyCombo::plain(Key::Left),
            KeyCombo::plain(Key::Right),
        ],
    },
    // idx 4: PAGE_NAV
    KeyBinding {
        key_short: "]/[",
        key: "] / [",
        desc_short: "Page",
        description: "Next/Previous page",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char(']')),
            KeyCombo::plain(Key::Char('[')),
        ],
    },
];

// =============================================================================
// SQL Modal
// =============================================================================

pub const SQL_MODAL_KEYS: &[KeyBinding] = &[
    // idx 0: SQL_RUN
    KeyBinding {
        key_short: "⌥Enter",
        key: "Alt+Enter",
        desc_short: "Run",
        description: "Execute query",
        action: Action::SqlModalSubmit,
        combos: &[KeyCombo::alt(Key::Enter)],
    },
    // idx 1: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseSqlModal,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 2: SQL_MOVE
    KeyBinding {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
            KeyCombo::plain(Key::Left),
            KeyCombo::plain(Key::Right),
        ],
    },
    // idx 3: HOME_END
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Line",
        description: "Line start/end",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Home), KeyCombo::plain(Key::End)],
    },
    // idx 4: TAB
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Tab/Complete",
        description: "Insert tab / Accept completion",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Tab)],
    },
    // idx 5: COMPLETION_TRIGGER
    KeyBinding {
        key_short: "^Space",
        key: "Ctrl+Space",
        desc_short: "Complete",
        description: "Trigger completion",
        action: Action::CompletionTrigger,
        combos: &[KeyCombo::ctrl(Key::Char(' '))],
    },
    // idx 6: CLEAR
    KeyBinding {
        key_short: "^L",
        key: "Ctrl+L",
        desc_short: "Clear",
        description: "Clear editor",
        action: Action::SqlModalClear,
        combos: &[KeyCombo::ctrl(Key::Char('l'))],
    },
];

// =============================================================================
// Overlays (common)
// =============================================================================

pub const OVERLAY_KEYS: &[KeyBinding] = &[
    // idx 0: ESC_CANCEL
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Close overlay / Cancel",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 1: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close overlay",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 2: ENTER_EXECUTE
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 3: ENTER_SELECT
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Confirm selection",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 4: NAVIGATE_JK
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate items",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
        ],
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
        combos: &[KeyCombo::plain(Key::Enter)],
    },
];

// =============================================================================
// Command Line
// =============================================================================

pub const COMMAND_LINE_KEYS: &[KeyBinding] = &[
    // idx 0
    KeyBinding {
        key_short: ":quit",
        key: ":quit",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[], // command-line commands, not key combos
    },
    // idx 1
    KeyBinding {
        key_short: ":help",
        key: ":help",
        desc_short: "Help",
        description: "Show help",
        action: Action::OpenHelp,
        combos: &[],
    },
    // idx 2
    KeyBinding {
        key_short: ":sql",
        key: ":sql",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
        combos: &[],
    },
    // idx 3
    KeyBinding {
        key_short: ":erd",
        key: ":erd",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::OpenErTablePicker,
        combos: &[],
    },
];

// =============================================================================
// Connection Setup
// =============================================================================

pub const CONNECTION_SETUP_KEYS: &[KeyBinding] = &[
    // idx 0: TAB_NAV
    KeyBinding {
        key_short: "Tab/⇧Tab",
        key: "Tab/⇧Tab",
        desc_short: "Next/Prev",
        description: "Next/Previous field",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Tab), KeyCombo::plain(Key::BackTab)],
    },
    // idx 1: TAB_NEXT
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Next",
        description: "Next field",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Tab)],
    },
    // idx 2: TAB_PREV
    KeyBinding {
        key_short: "⇧Tab",
        key: "⇧Tab",
        desc_short: "Prev",
        description: "Previous field",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::BackTab)],
    },
    // idx 3: SAVE
    KeyBinding {
        key_short: "^S",
        key: "Ctrl+S",
        desc_short: "Connect",
        description: "Save and connect",
        action: Action::ConnectionSetupSave,
        combos: &[KeyCombo::ctrl(Key::Char('s'))],
    },
    // idx 4: ESC_CANCEL
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        action: Action::ConnectionSetupCancel,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 5: ENTER_DROPDOWN
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Toggle",
        description: "Toggle dropdown (SSL field)",
        action: Action::ConnectionSetupToggleDropdown,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 6: DROPDOWN_NAV
    KeyBinding {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Select",
        description: "Dropdown navigation",
        action: Action::None,
        combos: &[KeyCombo::plain(Key::Up), KeyCombo::plain(Key::Down)],
    },
];

// =============================================================================
// Connection Error
// =============================================================================

pub const CONNECTION_ERROR_KEYS: &[KeyBinding] = &[
    // idx 0: EDIT
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection settings",
        action: Action::ReenterConnectionSetup,
        combos: &[KeyCombo::plain(Key::Char('e'))],
    },
    // idx 1: SWITCH
    KeyBinding {
        key_short: "s",
        key: "s",
        desc_short: "Switch",
        description: "Switch to another connection",
        action: Action::OpenConnectionSelector,
        combos: &[KeyCombo::plain(Key::Char('s'))],
    },
    // idx 2: DETAILS
    KeyBinding {
        key_short: "d",
        key: "d",
        desc_short: "Details",
        description: "Toggle error details",
        action: Action::ToggleConnectionErrorDetails,
        combos: &[KeyCombo::plain(Key::Char('d'))],
    },
    // idx 3: COPY
    KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Copy",
        description: "Copy error to clipboard",
        action: Action::CopyConnectionError,
        combos: &[KeyCombo::plain(Key::Char('c'))],
    },
    // idx 4: SCROLL (display-only)
    KeyBinding {
        key_short: "j/k",
        key: "j/k",
        desc_short: "Scroll",
        description: "Scroll error",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
        ],
    },
    // idx 5: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        action: Action::CloseConnectionError,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 6: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit",
        action: Action::Quit,
        combos: &[KeyCombo::plain(Key::Char('q'))],
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
        combos: &[KeyCombo::plain(Key::Up), KeyCombo::plain(Key::Down)],
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
        combos: &[KeyCombo::plain(Key::Up), KeyCombo::plain(Key::Down)],
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
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: NAVIGATE_JK (display-only)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j/k / ↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
        ],
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

// =============================================================================
// Help
// =============================================================================

pub const HELP_KEYS: &[KeyBinding] = &[
    // idx 0: HELP_SCROLL (display-only, covers both scroll directions)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        action: Action::HelpScrollDown,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
        ],
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

// =============================================================================
// Connections Mode (Explorer)
// =============================================================================

pub const CONNECTIONS_MODE_KEYS: &[KeyBinding] = &[
    // idx 0: CONNECT
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Connect",
        description: "Connect to selected",
        action: Action::ConfirmConnectionSelection,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: NEW
    KeyBinding {
        key_short: "n",
        key: "n",
        desc_short: "New",
        description: "New connection",
        action: Action::OpenConnectionSetup,
        combos: &[KeyCombo::plain(Key::Char('n'))],
    },
    // idx 2: EDIT
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection",
        action: Action::RequestEditSelectedConnection,
        combos: &[KeyCombo::plain(Key::Char('e'))],
    },
    // idx 3: DELETE
    KeyBinding {
        key_short: "d",
        key: "d / Del",
        desc_short: "Delete",
        description: "Delete connection",
        action: Action::RequestDeleteSelectedConnection,
        combos: &[
            KeyCombo::plain(Key::Char('d')),
            KeyCombo::plain(Key::Delete),
        ],
    },
    // idx 4: NAVIGATE (display-only)
    KeyBinding {
        key_short: "j/k",
        key: "j / k / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate list",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
        ],
    },
    // idx 5: HELP
    KeyBinding {
        key_short: "?",
        key: "?",
        desc_short: "Help",
        description: "Show help",
        action: Action::OpenHelp,
        combos: &[KeyCombo::plain(Key::Char('?'))],
    },
    // idx 6: TABLES
    KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Tables",
        description: "Switch to Tables mode",
        action: Action::ToggleExplorerMode,
        combos: &[KeyCombo::plain(Key::Char('c'))],
    },
    // idx 7: BACK
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Back",
        description: "Back to Tables mode",
        action: Action::ToggleExplorerMode,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 8: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[KeyCombo::plain(Key::Char('q'))],
    },
];

// =============================================================================
// Connection Selector
// =============================================================================

pub const CONNECTION_SELECTOR_KEYS: &[KeyBinding] = &[
    // idx 0: CONFIRM
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm selection",
        action: Action::ConfirmConnectionSelection,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: SELECT (display-only)
    KeyBinding {
        key_short: "↑/↓",
        key: "↑ / ↓ / j / k",
        desc_short: "Select",
        description: "Select connection",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Up),
            KeyCombo::plain(Key::Down),
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
        ],
    },
    // idx 2: NEW
    KeyBinding {
        key_short: "n",
        key: "n",
        desc_short: "New",
        description: "New connection",
        action: Action::OpenConnectionSetup,
        combos: &[KeyCombo::plain(Key::Char('n'))],
    },
    // idx 3: EDIT
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection",
        action: Action::RequestEditSelectedConnection,
        combos: &[KeyCombo::plain(Key::Char('e'))],
    },
    // idx 4: DELETE
    KeyBinding {
        key_short: "d",
        key: "d",
        desc_short: "Delete",
        description: "Delete connection",
        action: Action::RequestDeleteSelectedConnection,
        combos: &[KeyCombo::plain(Key::Char('d'))],
    },
    // idx 5: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[KeyCombo::plain(Key::Char('q'))],
    },
];

// =============================================================================
// Result Pane Active (Row/Cell selection)
// =============================================================================

pub const RESULT_ACTIVE_KEYS: &[KeyBinding] = &[
    // idx 0: ENTER_DEEPEN
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Enter row / cell selection",
        action: Action::ResultEnterRowActive,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: YANK
    KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy cell value to clipboard",
        action: Action::ResultCellYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    },
    // idx 2: STAGE_DELETE
    KeyBinding {
        key_short: "dd",
        key: "d, d",
        desc_short: "Stage Del",
        description: "Stage row for deletion (red highlight; :w to commit)",
        action: Action::StageRowForDelete,
        combos: &[], // dd is a two-key sequence, not a single combo
    },
    // idx 3: UNSTAGE_DELETE
    KeyBinding {
        key_short: "u",
        key: "u",
        desc_short: "Unstage",
        description: "Unstage last staged row",
        action: Action::UnstageLastStagedRow,
        combos: &[KeyCombo::plain(Key::Char('u'))],
    },
    // idx 4: CELL_NAV (display-only)
    KeyBinding {
        key_short: "h/l",
        key: "h / l",
        desc_short: "Cell",
        description: "Move cell left/right",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('h')),
            KeyCombo::plain(Key::Char('l')),
        ],
    },
    // idx 5: ROW_NAV (display-only)
    KeyBinding {
        key_short: "j/k",
        key: "j / k",
        desc_short: "Row",
        description: "Move row up/down",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Char('k')),
        ],
    },
    // idx 6: TOP_BOTTOM (display-only)
    KeyBinding {
        key_short: "g/G",
        key: "g / G",
        desc_short: "Top/Bot",
        description: "First/Last row",
        action: Action::None,
        combos: &[
            KeyCombo::plain(Key::Char('g')),
            KeyCombo::plain(Key::Char('G')),
        ],
    },
    // idx 7: ESC_BACK
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Back",
        description: "Exit to previous mode",
        action: Action::ResultExitToScroll,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 8: EDIT
    KeyBinding {
        key_short: "i",
        key: "i",
        desc_short: "Edit",
        description: "Edit active cell",
        action: Action::ResultEnterCellEdit,
        combos: &[KeyCombo::plain(Key::Char('i'))],
    },
    // idx 9: DRAFT_DISCARD
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Discard",
        description: "Discard pending draft and exit to Row Active",
        action: Action::ResultDiscardCellEdit,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

pub const CELL_EDIT_KEYS: &[KeyBinding] = &[
    // idx 0: WRITE
    KeyBinding {
        key_short: ":w",
        key: ":w",
        desc_short: "Write",
        description: "Preview and confirm UPDATE",
        action: Action::SubmitCellEditWrite,
        combos: &[], // :w is a command sequence, not a single combo
    },
    // idx 1: TYPE (display-only)
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Edit",
        description: "Edit cell value",
        action: Action::None,
        combos: &[],
    },
    // idx 2: COMMAND
    KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Open command line",
        action: Action::EnterCommandLine,
        combos: &[KeyCombo::plain(Key::Char(':'))],
    },
    // idx 3: ESC_CANCEL
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Exit to Cell Active (draft preserved)",
        action: Action::ResultCancelCellEdit,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

// =============================================================================
// Help Overlay Layout
// =============================================================================

/// Returns total line count of help overlay content.
///
/// Derived from the same section order as `HelpOverlay::render()`:
/// each section = 1 header + N key lines, separated by 1 blank line.
/// Sections: Global, Navigation, Result Pane, Cell Edit, SQL Editor,
/// Overlays, Command Line, Connection Setup, Connection Error,
/// Connections Mode, Connection Selector, ER Diagram Picker,
/// Table Picker, Command Palette, Help Overlay, Confirm Dialog.
pub const fn help_content_line_count() -> usize {
    // 16 sections × 1 header each = 16
    // 15 blank-line separators between sections = 15
    16 + 15
        + GLOBAL_KEYS.len()
        + NAVIGATION_KEYS.len()
        + RESULT_ACTIVE_KEYS.len()
        + CELL_EDIT_KEYS.len()
        + SQL_MODAL_KEYS.len()
        + OVERLAY_KEYS.len()
        + COMMAND_LINE_KEYS.len()
        + CONNECTION_SETUP_KEYS.len()
        + CONNECTION_ERROR_KEYS.len()
        + CONNECTIONS_MODE_KEYS.len()
        + CONNECTION_SELECTOR_KEYS.len()
        + ER_PICKER_KEYS.len()
        + TABLE_PICKER_KEYS.len()
        + COMMAND_PALETTE_KEYS.len()
        + HELP_KEYS.len()
        + CONFIRM_DIALOG_KEYS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that idx constants are valid indexes into their respective arrays.
    /// This catches errors when array entries are reordered or removed.
    #[test]
    fn idx_constants_are_within_bounds() {
        // GLOBAL_KEYS
        assert!(idx::global::QUIT < GLOBAL_KEYS.len());
        assert!(idx::global::HELP < GLOBAL_KEYS.len());
        assert!(idx::global::TABLE_PICKER < GLOBAL_KEYS.len());
        assert!(idx::global::PALETTE < GLOBAL_KEYS.len());
        assert!(idx::global::COMMAND_LINE < GLOBAL_KEYS.len());
        assert!(idx::global::FOCUS < GLOBAL_KEYS.len());
        assert!(idx::global::EXIT_FOCUS < GLOBAL_KEYS.len());
        assert!(idx::global::PANE_SWITCH < GLOBAL_KEYS.len());
        assert!(idx::global::INSPECTOR_TABS < GLOBAL_KEYS.len());
        assert!(idx::global::RELOAD < GLOBAL_KEYS.len());
        assert!(idx::global::SQL < GLOBAL_KEYS.len());
        assert!(idx::global::ER_DIAGRAM < GLOBAL_KEYS.len());
        assert!(idx::global::CONNECTIONS < GLOBAL_KEYS.len());

        // FOOTER_NAV_KEYS
        assert!(idx::footer_nav::SCROLL < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::SCROLL_SHORT < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::TOP_BOTTOM < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::H_SCROLL < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::PAGE_NAV < FOOTER_NAV_KEYS.len());

        // SQL_MODAL_KEYS
        assert!(idx::sql_modal::RUN < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::ESC_CLOSE < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::MOVE < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::HOME_END < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::TAB < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::COMPLETION_TRIGGER < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::CLEAR < SQL_MODAL_KEYS.len());

        // OVERLAY_KEYS
        assert!(idx::overlay::ESC_CANCEL < OVERLAY_KEYS.len());
        assert!(idx::overlay::ESC_CLOSE < OVERLAY_KEYS.len());
        assert!(idx::overlay::ENTER_EXECUTE < OVERLAY_KEYS.len());
        assert!(idx::overlay::ENTER_SELECT < OVERLAY_KEYS.len());
        assert!(idx::overlay::NAVIGATE_JK < OVERLAY_KEYS.len());
        assert!(idx::overlay::TYPE_FILTER < OVERLAY_KEYS.len());
        assert!(idx::overlay::ERROR_OPEN < OVERLAY_KEYS.len());

        // CONNECTION_SETUP_KEYS
        assert!(idx::conn_setup::TAB_NAV < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::TAB_NEXT < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::TAB_PREV < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::SAVE < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::ESC_CANCEL < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::ENTER_DROPDOWN < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::DROPDOWN_NAV < CONNECTION_SETUP_KEYS.len());

        // CONNECTION_ERROR_KEYS
        assert!(idx::conn_error::EDIT < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::DETAILS < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::COPY < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::SCROLL < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::ESC_CLOSE < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::QUIT < CONNECTION_ERROR_KEYS.len());

        // CONFIRM_DIALOG_KEYS
        assert!(idx::confirm::YES < CONFIRM_DIALOG_KEYS.len());
        assert!(idx::confirm::NO < CONFIRM_DIALOG_KEYS.len());

        // TABLE_PICKER_KEYS
        assert!(idx::table_picker::ENTER_SELECT < TABLE_PICKER_KEYS.len());
        assert!(idx::table_picker::NAVIGATE < TABLE_PICKER_KEYS.len());
        assert!(idx::table_picker::TYPE_FILTER < TABLE_PICKER_KEYS.len());
        assert!(idx::table_picker::ESC_CLOSE < TABLE_PICKER_KEYS.len());

        // ER_PICKER_KEYS
        assert!(idx::er_picker::ENTER_GENERATE < ER_PICKER_KEYS.len());
        assert!(idx::er_picker::NAVIGATE < ER_PICKER_KEYS.len());
        assert!(idx::er_picker::TYPE_FILTER < ER_PICKER_KEYS.len());
        assert!(idx::er_picker::ESC_CLOSE < ER_PICKER_KEYS.len());

        // COMMAND_PALETTE_KEYS
        assert!(idx::cmd_palette::ENTER_EXECUTE < COMMAND_PALETTE_KEYS.len());
        assert!(idx::cmd_palette::NAVIGATE_JK < COMMAND_PALETTE_KEYS.len());
        assert!(idx::cmd_palette::ESC_CLOSE < COMMAND_PALETTE_KEYS.len());

        // HELP_KEYS
        assert!(idx::help::SCROLL < HELP_KEYS.len());
        assert!(idx::help::CLOSE < HELP_KEYS.len());
        assert!(idx::help::QUIT < HELP_KEYS.len());

        // RESULT_ACTIVE_KEYS
        assert!(idx::result_active::ENTER_DEEPEN < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::YANK < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::STAGE_DELETE < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::UNSTAGE_DELETE < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::CELL_NAV < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::ROW_NAV < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::TOP_BOTTOM < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::ESC_BACK < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::EDIT < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::DRAFT_DISCARD < RESULT_ACTIVE_KEYS.len());

        // CELL_EDIT_KEYS
        assert!(idx::cell_edit::WRITE < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::TYPE < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::COMMAND < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::ESC_CANCEL < CELL_EDIT_KEYS.len());

        // CONNECTIONS_MODE_KEYS
        assert!(idx::connections_mode::CONNECT < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::NEW < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::EDIT < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::DELETE < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::NAVIGATE < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::HELP < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::TABLES < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::BACK < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::QUIT < CONNECTIONS_MODE_KEYS.len());

        // CONNECTION_SELECTOR_KEYS
        assert!(idx::connection_selector::CONFIRM < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::SELECT < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::NEW < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::EDIT < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::DELETE < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::QUIT < CONNECTION_SELECTOR_KEYS.len());
    }

    #[test]
    fn help_content_line_count_matches_section_structure() {
        // Build the same structure as HelpOverlay::render() and compare lengths.
        // Sections in render order (16 total):
        let sections: &[usize] = &[
            GLOBAL_KEYS.len(),
            NAVIGATION_KEYS.len(),
            RESULT_ACTIVE_KEYS.len(),
            CELL_EDIT_KEYS.len(),
            SQL_MODAL_KEYS.len(),
            OVERLAY_KEYS.len(),
            COMMAND_LINE_KEYS.len(),
            CONNECTION_SETUP_KEYS.len(),
            CONNECTION_ERROR_KEYS.len(),
            CONNECTIONS_MODE_KEYS.len(),
            CONNECTION_SELECTOR_KEYS.len(),
            ER_PICKER_KEYS.len(),
            TABLE_PICKER_KEYS.len(),
            COMMAND_PALETTE_KEYS.len(),
            HELP_KEYS.len(),
            CONFIRM_DIALOG_KEYS.len(),
        ];
        let section_count = sections.len();
        // 1 header per section + entries + (section_count - 1) blank separators
        let expected: usize = section_count + sections.iter().sum::<usize>() + (section_count - 1);

        assert_eq!(help_content_line_count(), expected);
    }
}
