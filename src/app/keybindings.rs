//! Centralized keybinding definitions.
//! Single source of truth for key/description used by Footer, Help, and Palette.

use super::action::Action;

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
    pub action: Action,
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
    }

    pub mod sql_modal {
        pub const RUN: usize = 0;
        pub const ESC_CLOSE: usize = 1;
        pub const MOVE: usize = 2;
    }

    pub mod overlay {
        pub const ESC_CANCEL: usize = 0;
        pub const ESC_CLOSE: usize = 1;
        pub const ENTER_EXECUTE: usize = 2;
        pub const ENTER_SELECT: usize = 3;
        pub const NAVIGATE: usize = 4;
        pub const NAVIGATE_JK: usize = 5;
        pub const TYPE_FILTER: usize = 6;
        pub const ERROR_OPEN: usize = 7;
    }

    pub mod conn_setup {
        pub const TAB_NAV: usize = 0;
        pub const TAB_NEXT: usize = 1;
        pub const TAB_PREV: usize = 2;
        pub const SAVE: usize = 3;
        pub const ESC_CANCEL: usize = 4;
    }

    pub mod conn_error {
        pub const EDIT: usize = 0;
        pub const DETAILS: usize = 1;
        pub const COPY: usize = 2;
        pub const SCROLL: usize = 3;
        pub const ESC_CLOSE: usize = 4;
        pub const QUIT: usize = 5;
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
    },
    // idx 1: HELP
    KeyBinding {
        key_short: "?",
        key: "?",
        desc_short: "Help",
        description: "Toggle help",
        action: Action::OpenHelp,
    },
    // idx 2: TABLE_PICKER
    KeyBinding {
        key_short: "^P",
        key: "Ctrl+P",
        desc_short: "Tables",
        description: "Open Table Picker",
        action: Action::OpenTablePicker,
    },
    // idx 3: PALETTE
    KeyBinding {
        key_short: "^K",
        key: "Ctrl+K",
        desc_short: "Palette",
        description: "Open Command Palette",
        action: Action::OpenCommandPalette,
    },
    // idx 4: COMMAND_LINE
    KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Enter command line",
        action: Action::EnterCommandLine,
    },
    // idx 5: FOCUS
    KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Focus",
        description: "Toggle Focus mode",
        action: Action::ToggleFocus,
    },
    // idx 6: EXIT_FOCUS (same key, different display)
    KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Exit Focus",
        description: "Exit Focus mode",
        action: Action::ToggleFocus,
    },
    // idx 7: PANE_SWITCH
    KeyBinding {
        key_short: "1/2/3",
        key: "1/2/3",
        desc_short: "Pane",
        description: "Switch pane focus",
        action: Action::None,
    },
    // idx 8: INSPECTOR_TABS
    KeyBinding {
        key_short: "Tab/⇧Tab",
        key: "Tab/⇧Tab",
        desc_short: "InsTabs",
        description: "Inspector prev/next tab",
        action: Action::None,
    },
    // idx 9: RELOAD
    KeyBinding {
        key_short: "r",
        key: "r",
        desc_short: "Reload",
        description: "Reload metadata",
        action: Action::ReloadMetadata,
    },
    // idx 10: SQL
    KeyBinding {
        key_short: "s",
        key: "s",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
    },
    // idx 11: ER_DIAGRAM
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::ErOpenDiagram,
    },
    // idx 12: CONNECTIONS
    KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Connections",
        description: "Toggle Connections mode",
        action: Action::ToggleExplorerMode,
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
    },
    KeyBinding {
        key_short: "k",
        key: "k / ↑",
        desc_short: "Up",
        description: "Move up / scroll",
        action: Action::None,
    },
    KeyBinding {
        key_short: "g",
        key: "g / Home",
        desc_short: "Top",
        description: "First item / top",
        action: Action::None,
    },
    KeyBinding {
        key_short: "G",
        key: "G / End",
        desc_short: "Bottom",
        description: "Last item / bottom",
        action: Action::None,
    },
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
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
    },
    // idx 1: SCROLL_SHORT (same as SCROLL for now)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
    },
    // idx 2: TOP_BOTTOM
    KeyBinding {
        key_short: "g/G",
        key: "g / G",
        desc_short: "Top/Bottom",
        description: "First/Last item",
        action: Action::None,
    },
    // idx 3: H_SCROLL
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l / ← / →",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
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
    },
    // idx 1: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseSqlModal,
    },
    // idx 2: SQL_MOVE
    KeyBinding {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
    },
    // idx 3: HOME_END
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Line",
        description: "Line start/end",
        action: Action::None,
    },
    // idx 4: TAB
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Tab/Complete",
        description: "Insert tab / Accept completion",
        action: Action::None,
    },
    // idx 5: COMPLETION_TRIGGER
    KeyBinding {
        key_short: "^Space",
        key: "Ctrl+Space",
        desc_short: "Complete",
        description: "Trigger completion",
        action: Action::CompletionTrigger,
    },
    // idx 6: CLEAR
    KeyBinding {
        key_short: "^L",
        key: "Ctrl+L",
        desc_short: "Clear",
        description: "Clear editor",
        action: Action::SqlModalClear,
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
    },
    // idx 1: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close overlay",
        action: Action::None,
    },
    // idx 2: ENTER_EXECUTE
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
    },
    // idx 3: ENTER_SELECT
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Confirm selection",
        action: Action::None,
    },
    // idx 4: NAVIGATE
    KeyBinding {
        key_short: "↑↓",
        key: "↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate items",
        action: Action::None,
    },
    // idx 5: NAVIGATE_JK
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate items",
        action: Action::None,
    },
    // idx 6: TYPE_FILTER
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
    },
    // idx 7: ERROR_OPEN
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Error",
        description: "View error details",
        action: Action::None,
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
    },
    // idx 1
    KeyBinding {
        key_short: ":help",
        key: ":help",
        desc_short: "Help",
        description: "Show help",
        action: Action::OpenHelp,
    },
    // idx 2
    KeyBinding {
        key_short: ":sql",
        key: ":sql",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
    },
    // idx 3
    KeyBinding {
        key_short: ":erd",
        key: ":erd",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::ErOpenDiagram,
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
    },
    // idx 1: TAB_NEXT
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Next",
        description: "Next field",
        action: Action::None,
    },
    // idx 2: TAB_PREV
    KeyBinding {
        key_short: "⇧Tab",
        key: "⇧Tab",
        desc_short: "Prev",
        description: "Previous field",
        action: Action::None,
    },
    // idx 3: SAVE
    KeyBinding {
        key_short: "^S",
        key: "Ctrl+S",
        desc_short: "Save",
        description: "Save and connect",
        action: Action::ConnectionSetupSave,
    },
    // idx 4: ESC_CANCEL
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        action: Action::ConnectionSetupCancel,
    },
    // idx 5: ENTER_DROPDOWN
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Toggle",
        description: "Toggle dropdown (SSL field)",
        action: Action::ConnectionSetupToggleDropdown,
    },
    // idx 6: DROPDOWN_NAV
    KeyBinding {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Select",
        description: "Dropdown navigation",
        action: Action::None,
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
    },
    // idx 1: DETAILS
    KeyBinding {
        key_short: "d",
        key: "d",
        desc_short: "Details",
        description: "Toggle error details",
        action: Action::ToggleConnectionErrorDetails,
    },
    // idx 2: COPY
    KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Copy",
        description: "Copy error to clipboard",
        action: Action::CopyConnectionError,
    },
    // idx 3: SCROLL
    KeyBinding {
        key_short: "j/k",
        key: "j/k",
        desc_short: "Scroll",
        description: "Scroll error",
        action: Action::None,
    },
    // idx 4: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        action: Action::CloseConnectionError,
    },
    // idx 5: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit",
        action: Action::Quit,
    },
];

// =============================================================================
// Confirm Dialog
// =============================================================================

pub const CONFIRM_DIALOG_KEYS: &[KeyBinding] = &[
    // idx 0: CONFIRM
    KeyBinding {
        key_short: "Enter/y",
        key: "Enter / y",
        desc_short: "Yes",
        description: "Confirm",
        action: Action::ConfirmDialogConfirm,
    },
    // idx 1: CANCEL
    KeyBinding {
        key_short: "Esc/n",
        key: "Esc / n",
        desc_short: "No",
        description: "Cancel",
        action: Action::ConfirmDialogCancel,
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
    },
    // idx 1: NAVIGATE
    KeyBinding {
        key_short: "↑↓",
        key: "↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        action: Action::None,
    },
    // idx 2: TYPE_FILTER
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
    },
    // idx 3: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        action: Action::CloseTablePicker,
    },
];

// =============================================================================
// Command Palette
// =============================================================================

pub const COMMAND_PALETTE_KEYS: &[KeyBinding] = &[
    // idx 0: ENTER_EXECUTE
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
    },
    // idx 1: NAVIGATE_JK
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j/k / ↑↓",
        desc_short: "Navigate",
        description: "Navigate",
        action: Action::None,
    },
    // idx 2: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        action: Action::CloseCommandPalette,
    },
];

// =============================================================================
// Help
// =============================================================================

pub const HELP_KEYS: &[KeyBinding] = &[
    // idx 0: HELP_SCROLL
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        action: Action::HelpScrollDown,
    },
    // idx 1: HELP_CLOSE
    KeyBinding {
        key_short: "?/Esc",
        key: "? / Esc",
        desc_short: "Close",
        description: "Close help",
        action: Action::CloseHelp,
    },
    // idx 2: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit",
        action: Action::Quit,
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
    },
    // idx 1: NEW
    KeyBinding {
        key_short: "n",
        key: "n",
        desc_short: "New",
        description: "New connection",
        action: Action::OpenConnectionSetup,
    },
    // idx 2: EDIT
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection",
        action: Action::RequestEditSelectedConnection,
    },
    // idx 3: DELETE
    KeyBinding {
        key_short: "d",
        key: "d / Del",
        desc_short: "Delete",
        description: "Delete connection",
        action: Action::RequestDeleteSelectedConnection,
    },
    // idx 4: NAVIGATE
    KeyBinding {
        key_short: "j/k",
        key: "j / k / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate list",
        action: Action::None,
    },
    // idx 5: HELP
    KeyBinding {
        key_short: "?",
        key: "?",
        desc_short: "Help",
        description: "Show help",
        action: Action::OpenHelp,
    },
    // idx 6: TABLES
    KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Tables",
        description: "Switch to Tables mode",
        action: Action::ToggleExplorerMode,
    },
    // idx 7: BACK
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Back",
        description: "Back to Tables mode",
        action: Action::ToggleExplorerMode,
    },
    // idx 8: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
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
    },
    // idx 1: SELECT
    KeyBinding {
        key_short: "↑/↓",
        key: "↑ / ↓ / j / k",
        desc_short: "Select",
        description: "Select connection",
        action: Action::None,
    },
    // idx 2: NEW
    KeyBinding {
        key_short: "n",
        key: "n",
        desc_short: "New",
        description: "New connection",
        action: Action::OpenConnectionSetup,
    },
    // idx 3: EDIT
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection",
        action: Action::RequestEditSelectedConnection,
    },
    // idx 4: DELETE
    KeyBinding {
        key_short: "d",
        key: "d",
        desc_short: "Delete",
        description: "Delete connection",
        action: Action::RequestDeleteSelectedConnection,
    },
    // idx 5: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
    },
];

// =============================================================================
// Help Overlay Layout
// =============================================================================

/// Total lines in help overlay content (8 sections + 7 blank lines + key entries)
pub const HELP_TOTAL_LINES: usize = 8
    + 7
    + GLOBAL_KEYS.len()
    + NAVIGATION_KEYS.len()
    + SQL_MODAL_KEYS.len()
    + OVERLAY_KEYS.len()
    + COMMAND_LINE_KEYS.len()
    + CONNECTION_SETUP_KEYS.len()
    + CONNECTION_ERROR_KEYS.len()
    + CONFIRM_DIALOG_KEYS.len();

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

        // SQL_MODAL_KEYS
        assert!(idx::sql_modal::RUN < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::ESC_CLOSE < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::MOVE < SQL_MODAL_KEYS.len());

        // OVERLAY_KEYS
        assert!(idx::overlay::ESC_CANCEL < OVERLAY_KEYS.len());
        assert!(idx::overlay::ESC_CLOSE < OVERLAY_KEYS.len());
        assert!(idx::overlay::ENTER_EXECUTE < OVERLAY_KEYS.len());
        assert!(idx::overlay::ENTER_SELECT < OVERLAY_KEYS.len());
        assert!(idx::overlay::NAVIGATE < OVERLAY_KEYS.len());
        assert!(idx::overlay::NAVIGATE_JK < OVERLAY_KEYS.len());
        assert!(idx::overlay::TYPE_FILTER < OVERLAY_KEYS.len());
        assert!(idx::overlay::ERROR_OPEN < OVERLAY_KEYS.len());

        // CONNECTION_SETUP_KEYS
        assert!(idx::conn_setup::TAB_NAV < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::TAB_NEXT < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::TAB_PREV < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::SAVE < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::ESC_CANCEL < CONNECTION_SETUP_KEYS.len());

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

        // COMMAND_PALETTE_KEYS
        assert!(idx::cmd_palette::ENTER_EXECUTE < COMMAND_PALETTE_KEYS.len());
        assert!(idx::cmd_palette::NAVIGATE_JK < COMMAND_PALETTE_KEYS.len());
        assert!(idx::cmd_palette::ESC_CLOSE < COMMAND_PALETTE_KEYS.len());

        // HELP_KEYS
        assert!(idx::help::SCROLL < HELP_KEYS.len());
        assert!(idx::help::CLOSE < HELP_KEYS.len());
        assert!(idx::help::QUIT < HELP_KEYS.len());

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
}
