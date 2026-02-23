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
    /// The key combinations that trigger this binding (executable triggers only).
    ///
    /// `keymap::resolve()` matches incoming events against these combos.
    /// In **display-only arrays** (those never passed to `keymap::resolve()`:
    /// `NAVIGATION_KEYS`, `FOOTER_NAV_KEYS`, `SQL_MODAL_KEYS`, `OVERLAY_KEYS`,
    /// `CONNECTION_SETUP_KEYS`, `RESULT_ACTIVE_KEYS`, `CONNECTIONS_MODE_KEYS`),
    /// all `Action::None` entries must have `combos: &[]`, because non-empty
    /// combos on a display-only entry create a false impression of being an
    /// executable trigger when they are never matched at runtime.
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
        combos: &[],
    },
    KeyBinding {
        key_short: "k",
        key: "k / ↑",
        desc_short: "Up",
        description: "Move up / scroll",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "g",
        key: "g / Home",
        desc_short: "Top",
        description: "First item / top",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "G",
        key: "G / End",
        desc_short: "Bottom",
        description: "Last item / bottom",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "^D/^U",
        key: "Ctrl+D / Ctrl+U",
        desc_short: "Half Page",
        description: "Scroll half page down/up",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "^F/^B",
        key: "Ctrl+F/B / PgDn/Up",
        desc_short: "Full Page",
        description: "Scroll full page down/up",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
        combos: &[],
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
        combos: &[],
    },
    // idx 1: SCROLL_SHORT (same as SCROLL for now)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
        combos: &[],
    },
    // idx 2: TOP_BOTTOM
    KeyBinding {
        key_short: "g/G",
        key: "g / G",
        desc_short: "Top/Bottom",
        description: "First/Last item",
        action: Action::None,
        combos: &[],
    },
    // idx 3: H_SCROLL
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l / ← / →",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
        combos: &[],
    },
    // idx 4: PAGE_NAV
    KeyBinding {
        key_short: "]/[",
        key: "] / [",
        desc_short: "Page",
        description: "Next/Previous page",
        action: Action::None,
        combos: &[],
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
        combos: &[],
    },
    // idx 3: HOME_END
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Line",
        description: "Line start/end",
        action: Action::None,
        combos: &[],
    },
    // idx 4: TAB
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Tab/Complete",
        description: "Insert tab / Accept completion",
        action: Action::None,
        combos: &[],
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
    // idx 4: SUBMIT (executable, not displayed)
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Submit",
        description: "Submit command",
        action: Action::CommandLineSubmit,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 5: EXIT (executable, not displayed)
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Exit",
        description: "Exit command line",
        action: Action::ExitCommandLine,
        combos: &[KeyCombo::plain(Key::Esc)],
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
        combos: &[],
    },
    // idx 1: TAB_NEXT
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Next",
        description: "Next field",
        action: Action::None,
        combos: &[],
    },
    // idx 2: TAB_PREV
    KeyBinding {
        key_short: "⇧Tab",
        key: "⇧Tab",
        desc_short: "Prev",
        description: "Previous field",
        action: Action::None,
        combos: &[],
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
        combos: &[],
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
    // Executable entries for scroll (idx 4 SCROLL is display-only)
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::ScrollConnectionErrorDown,
        combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::ScrollConnectionErrorUp,
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
    // Executable entries (idx 1 NAVIGATE is display-only; Up/Down only, j/k are filter input)
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
    // Executable entries (idx 3 NAVIGATE is display-only; Up/Down only, j/k are filter input)
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
    // Executable entries (idx 0 and 1 are display-only)
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
// Help
// =============================================================================

pub const HELP_KEYS: &[KeyBinding] = &[
    // idx 0: HELP_SCROLL (display-only; executable entries appended below)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        action: Action::None,
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
    // Executable entries (not shown in Footer/Help display, used by keymap::resolve)
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
        combos: &[],
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
    // Executable entries (idx 1 SELECT is display-only)
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::ConnectionListSelectNext,
        combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
    },
    KeyBinding {
        key_short: "",
        key: "",
        desc_short: "",
        description: "",
        action: Action::ConnectionListSelectPrevious,
        combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
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
        combos: &[],
    },
    // idx 5: ROW_NAV (display-only)
    KeyBinding {
        key_short: "j/k",
        key: "j / k",
        desc_short: "Row",
        description: "Move row up/down",
        action: Action::None,
        combos: &[],
    },
    // idx 6: TOP_BOTTOM (display-only)
    KeyBinding {
        key_short: "g/G",
        key: "g / G",
        desc_short: "Top/Bot",
        description: "First/Last row",
        action: Action::None,
        combos: &[],
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

// =============================================================================
// Predicate functions for Normal mode routing
// =============================================================================
//
// These are thin wrappers over GLOBAL_KEYS binding combos.
// They are the single source of truth for what key triggers each global action.
// Phase 4 semantic tests verify that GLOBAL_KEYS[idx] has the expected action,
// catching bugs if the array is ever reordered.

pub fn is_quit(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::QUIT].combos.contains(combo)
}

pub fn is_help(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::HELP].combos.contains(combo)
}

pub fn is_table_picker(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::TABLE_PICKER]
        .combos
        .contains(combo)
}

pub fn is_command_palette(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::PALETTE].combos.contains(combo)
}

pub fn is_command_line(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::COMMAND_LINE]
        .combos
        .contains(combo)
}

pub fn is_focus_toggle(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::FOCUS].combos.contains(combo)
}

pub fn is_reload(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::RELOAD].combos.contains(combo)
}

pub fn is_open_sql(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::SQL].combos.contains(combo)
}

pub fn is_open_er(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::ER_DIAGRAM].combos.contains(combo)
}

pub fn is_toggle_connections(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::CONNECTIONS].combos.contains(combo)
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

    /// Semantic consistency tests (#126)
    ///
    /// These tests prevent keybinding drift — they verify that:
    /// - idx constants point to the correct Action
    /// - Non-None bindings always have at least one combo
    /// - No duplicate combos within simple (non-context-dependent) modes
    /// - keymap::resolve() round-trips every non-None non-empty-combo entry
    /// - Char-input modes have no executable plain Char(_) combos that would mask filter input
    mod semantic {
        use super::*;
        use crate::app::keymap;
        use rstest::rstest;

        // ------------------------------------------------------------------ //
        // 1. idx-to-Action correctness
        // ------------------------------------------------------------------ //

        #[rstest]
        #[case(idx::global::QUIT, Action::Quit)]
        #[case(idx::global::HELP, Action::OpenHelp)]
        #[case(idx::global::TABLE_PICKER, Action::OpenTablePicker)]
        #[case(idx::global::PALETTE, Action::OpenCommandPalette)]
        #[case(idx::global::COMMAND_LINE, Action::EnterCommandLine)]
        #[case(idx::global::RELOAD, Action::ReloadMetadata)]
        #[case(idx::global::SQL, Action::OpenSqlModal)]
        #[case(idx::global::ER_DIAGRAM, Action::OpenErTablePicker)]
        #[case(idx::global::CONNECTIONS, Action::ToggleExplorerMode)]
        fn global_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&GLOBAL_KEYS[i].action) == std::mem::discriminant(&expected),
                "GLOBAL_KEYS[{i}] has action {:?}, expected {expected:?}",
                GLOBAL_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::help::CLOSE, Action::CloseHelp)]
        #[case(idx::help::QUIT, Action::Quit)]
        fn help_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&HELP_KEYS[i].action) == std::mem::discriminant(&expected),
                "HELP_KEYS[{i}] has action {:?}, expected {expected:?}",
                HELP_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::confirm::YES, Action::ConfirmDialogConfirm)]
        #[case(idx::confirm::NO, Action::ConfirmDialogCancel)]
        fn confirm_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&CONFIRM_DIALOG_KEYS[i].action)
                    == std::mem::discriminant(&expected),
                "CONFIRM_DIALOG_KEYS[{i}] has action {:?}, expected {expected:?}",
                CONFIRM_DIALOG_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::conn_error::QUIT, Action::Quit)]
        #[case(idx::conn_error::ESC_CLOSE, Action::CloseConnectionError)]
        #[case(idx::conn_error::EDIT, Action::ReenterConnectionSetup)]
        #[case(idx::conn_error::SWITCH, Action::OpenConnectionSelector)]
        #[case(idx::conn_error::DETAILS, Action::ToggleConnectionErrorDetails)]
        #[case(idx::conn_error::COPY, Action::CopyConnectionError)]
        fn conn_error_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&CONNECTION_ERROR_KEYS[i].action)
                    == std::mem::discriminant(&expected),
                "CONNECTION_ERROR_KEYS[{i}] has action {:?}, expected {expected:?}",
                CONNECTION_ERROR_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::connection_selector::CONFIRM, Action::ConfirmConnectionSelection)]
        #[case(idx::connection_selector::NEW, Action::OpenConnectionSetup)]
        #[case(idx::connection_selector::EDIT, Action::RequestEditSelectedConnection)]
        #[case(
            idx::connection_selector::DELETE,
            Action::RequestDeleteSelectedConnection
        )]
        #[case(idx::connection_selector::QUIT, Action::Quit)]
        fn connection_selector_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&CONNECTION_SELECTOR_KEYS[i].action)
                    == std::mem::discriminant(&expected),
                "CONNECTION_SELECTOR_KEYS[{i}] has action {:?}, expected {expected:?}",
                CONNECTION_SELECTOR_KEYS[i].action
            );
        }

        // ------------------------------------------------------------------ //
        // 2. Non-None bindings have at least one combo
        // ------------------------------------------------------------------ //

        fn check_non_none_have_combos(bindings: &[KeyBinding], name: &str) {
            for (i, kb) in bindings.iter().enumerate() {
                if !matches!(kb.action, Action::None) && kb.combos.is_empty() {
                    // command-line text commands (:quit, :help, etc.) legitimately have no combos
                    if kb.key.starts_with(':') {
                        continue;
                    }
                    // :w command sequence also has no combo
                    if kb.key_short == ":w" || kb.desc_short == "Write" {
                        continue;
                    }
                    panic!(
                        "{name}[{i}] has action {:?} but no combos (key={:?})",
                        kb.action, kb.key
                    );
                }
            }
        }

        #[test]
        fn all_non_none_bindings_have_combos() {
            check_non_none_have_combos(GLOBAL_KEYS, "GLOBAL_KEYS");
            check_non_none_have_combos(HELP_KEYS, "HELP_KEYS");
            check_non_none_have_combos(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_non_none_have_combos(CONNECTION_ERROR_KEYS, "CONNECTION_ERROR_KEYS");
            check_non_none_have_combos(CONNECTION_SELECTOR_KEYS, "CONNECTION_SELECTOR_KEYS");
            check_non_none_have_combos(COMMAND_PALETTE_KEYS, "COMMAND_PALETTE_KEYS");
            check_non_none_have_combos(TABLE_PICKER_KEYS, "TABLE_PICKER_KEYS");
            check_non_none_have_combos(ER_PICKER_KEYS, "ER_PICKER_KEYS");
            check_non_none_have_combos(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
            check_non_none_have_combos(CELL_EDIT_KEYS, "CELL_EDIT_KEYS");
        }

        // ------------------------------------------------------------------ //
        // 3. No duplicate combos within simple (non-context-dependent) modes
        // ------------------------------------------------------------------ //
        //
        // Normal mode is excluded: context-dependent keys intentionally share
        // combos across different actions (e.g., 'j' means ScrollDown in result
        // pane but SelectNext in explorer).

        fn check_no_duplicate_combos(bindings: &[KeyBinding], name: &str) {
            let mut seen: Vec<KeyCombo> = Vec::new();
            for kb in bindings
                .iter()
                .filter(|kb| !matches!(kb.action, Action::None))
            {
                for combo in kb.combos {
                    if seen.contains(combo) {
                        panic!(
                            "{name}: duplicate combo {combo:?} in binding {:?}",
                            kb.action
                        );
                    }
                    seen.push(*combo);
                }
            }
        }

        #[test]
        fn no_duplicate_combos_in_simple_modes() {
            check_no_duplicate_combos(HELP_KEYS, "HELP_KEYS");
            check_no_duplicate_combos(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_no_duplicate_combos(CONNECTION_ERROR_KEYS, "CONNECTION_ERROR_KEYS");
            check_no_duplicate_combos(CONNECTION_SELECTOR_KEYS, "CONNECTION_SELECTOR_KEYS");
            check_no_duplicate_combos(COMMAND_PALETTE_KEYS, "COMMAND_PALETTE_KEYS");
            check_no_duplicate_combos(TABLE_PICKER_KEYS, "TABLE_PICKER_KEYS");
            check_no_duplicate_combos(ER_PICKER_KEYS, "ER_PICKER_KEYS");
            check_no_duplicate_combos(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
        }

        // ------------------------------------------------------------------ //
        // 4. keymap::resolve() round-trip
        // ------------------------------------------------------------------ //

        fn check_keymap_roundtrip(bindings: &[KeyBinding], name: &str) {
            for kb in bindings
                .iter()
                .filter(|kb| !matches!(kb.action, Action::None))
            {
                for combo in kb.combos {
                    let resolved = keymap::resolve(combo, bindings);
                    match resolved {
                        Some(ref action)
                            if std::mem::discriminant(action)
                                == std::mem::discriminant(&kb.action) => {}
                        other => panic!(
                            "{name}: combo {combo:?} resolved to {other:?}, expected {:?}",
                            kb.action
                        ),
                    }
                }
            }
        }

        #[test]
        fn keymap_resolve_roundtrip_for_simple_modes() {
            check_keymap_roundtrip(HELP_KEYS, "HELP_KEYS");
            check_keymap_roundtrip(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_keymap_roundtrip(CONNECTION_ERROR_KEYS, "CONNECTION_ERROR_KEYS");
            check_keymap_roundtrip(CONNECTION_SELECTOR_KEYS, "CONNECTION_SELECTOR_KEYS");
            check_keymap_roundtrip(COMMAND_PALETTE_KEYS, "COMMAND_PALETTE_KEYS");
            check_keymap_roundtrip(TABLE_PICKER_KEYS, "TABLE_PICKER_KEYS");
            check_keymap_roundtrip(ER_PICKER_KEYS, "ER_PICKER_KEYS");
            check_keymap_roundtrip(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
        }

        // ------------------------------------------------------------------ //
        // 5. Char fallback safety
        // ------------------------------------------------------------------ //
        //
        // Modes with freeform character input (TablePicker, ErTablePicker,
        // CommandLine, CellEdit) must not have executable plain Char(_) combos
        // in their key arrays, because those would shadow the filter/edit input.
        //
        // Exception: CellEdit has Char(':') for EnterCommandLine — this is
        // intentional (it opens command line, not inserts ':' as edit text).

        fn check_no_plain_char_in_filter_mode(
            bindings: &[KeyBinding],
            name: &str,
            allowed_chars: &[char],
        ) {
            let no_mods = Modifiers {
                ctrl: false,
                alt: false,
                shift: false,
            };
            for kb in bindings
                .iter()
                .filter(|kb| !matches!(kb.action, Action::None))
            {
                for combo in kb.combos {
                    if combo.modifiers == no_mods
                        && let Key::Char(c) = combo.key
                    {
                        assert!(
                            allowed_chars.contains(&c),
                            "{name}: executable entry {:?} has plain Char({c:?}) combo \
                             which would shadow filter input",
                            kb.action
                        );
                    }
                }
            }
        }

        #[test]
        fn table_picker_has_no_plain_char_combos() {
            // TablePicker uses Up/Down for nav (not j/k), so no plain Char combos expected
            check_no_plain_char_in_filter_mode(TABLE_PICKER_KEYS, "TABLE_PICKER_KEYS", &[]);
        }

        #[test]
        fn er_picker_has_no_plain_char_combos() {
            // ErPicker: Space toggles selection (intentional command, not text input),
            // Ctrl+A selects all (has Ctrl modifier). Space is the only plain Char allowed.
            check_no_plain_char_in_filter_mode(ER_PICKER_KEYS, "ER_PICKER_KEYS", &[' ']);
        }

        #[test]
        fn command_line_has_no_problematic_plain_char_combos() {
            // CommandLine: Enter and Esc are non-Char keys. No plain Char combos expected.
            check_no_plain_char_in_filter_mode(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS", &[]);
        }

        #[test]
        fn cell_edit_plain_char_combos_are_intentional() {
            // CellEdit: Char(':') for EnterCommandLine is intentional.
            // Verify only ':' appears as a plain Char combo.
            check_no_plain_char_in_filter_mode(CELL_EDIT_KEYS, "CELL_EDIT_KEYS", &[':']);
        }

        // ------------------------------------------------------------------ //
        // 6. Display-only arrays: Action::None entries must have no combos
        // ------------------------------------------------------------------ //
        //
        // In display-only arrays (never passed to keymap::resolve()), combos
        // serve no runtime purpose and their presence creates a false impression
        // that they are executable triggers. All Action::None entries in these
        // arrays must have combos: &[].

        fn check_none_action_entries_have_no_combos(bindings: &[KeyBinding], name: &str) {
            for (i, kb) in bindings.iter().enumerate() {
                if matches!(kb.action, Action::None) && !kb.combos.is_empty() {
                    panic!(
                        "{name}[{i}] has action Action::None but non-empty combos: {:?}",
                        kb.combos
                    );
                }
            }
        }

        #[test]
        fn display_only_array_entries_have_no_combos() {
            check_none_action_entries_have_no_combos(NAVIGATION_KEYS, "NAVIGATION_KEYS");
            check_none_action_entries_have_no_combos(FOOTER_NAV_KEYS, "FOOTER_NAV_KEYS");
            check_none_action_entries_have_no_combos(SQL_MODAL_KEYS, "SQL_MODAL_KEYS");
            check_none_action_entries_have_no_combos(OVERLAY_KEYS, "OVERLAY_KEYS");
            check_none_action_entries_have_no_combos(
                CONNECTION_SETUP_KEYS,
                "CONNECTION_SETUP_KEYS",
            );
            check_none_action_entries_have_no_combos(RESULT_ACTIVE_KEYS, "RESULT_ACTIVE_KEYS");
            check_none_action_entries_have_no_combos(
                CONNECTIONS_MODE_KEYS,
                "CONNECTIONS_MODE_KEYS",
            );
        }
    }
}
