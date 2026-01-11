//! Centralized keybinding definitions.
//! Single source of truth for key/description used by Footer, Help, and Palette.

use super::action::Action;

#[derive(Clone)]
pub struct KeyBinding {
    /// Display key for Help/Palette (e.g., "Ctrl+P")
    pub key: &'static str,
    /// Full description for Help/Palette
    pub description: &'static str,
    #[allow(dead_code)]
    pub action: Action,
}

// =============================================================================
// Footer Hints (short form for status bar)
// =============================================================================

pub mod footer {
    pub const QUIT: (&str, &str) = ("q", "Quit");
    pub const HELP: (&str, &str) = ("?", "Help");
    pub const RELOAD: (&str, &str) = ("r", "Reload");
    pub const SQL: (&str, &str) = ("s", "SQL");
    pub const ER_DIAGRAM: (&str, &str) = ("e", "ER Diagram");
    pub const CONNECT: (&str, &str) = ("c", "Connect");
    pub const TABLE_PICKER: (&str, &str) = ("^P", "Tables");
    pub const PALETTE: (&str, &str) = ("^K", "Palette");
    pub const FOCUS: (&str, &str) = ("f", "Focus");
    pub const EXIT_FOCUS: (&str, &str) = ("f", "Exit Focus");
    pub const PANE_SWITCH: (&str, &str) = ("1/2/3", "Pane");
    pub const SCROLL: (&str, &str) = ("j/k/g/G", "Scroll");
    pub const SCROLL_SHORT: (&str, &str) = ("j/k", "Scroll");
    pub const H_SCROLL: (&str, &str) = ("h/l", "H-Scroll");
    pub const TOP_BOTTOM: (&str, &str) = ("g/G", "Top/Bottom");
    pub const INSPECTOR_TABS: (&str, &str) = ("Tab/⇧Tab", "InsTabs");
    pub const ERROR_OPEN: (&str, &str) = ("Enter", "Error");

    // Overlays
    pub const ENTER_EXECUTE: (&str, &str) = ("Enter", "Execute");
    pub const ENTER_SELECT: (&str, &str) = ("Enter", "Select");
    pub const ESC_CANCEL: (&str, &str) = ("Esc", "Cancel");
    pub const ESC_CLOSE: (&str, &str) = ("Esc", "Close");
    pub const NAVIGATE: (&str, &str) = ("↑↓", "Navigate");
    pub const NAVIGATE_JK: (&str, &str) = ("j/k / ↑↓", "Navigate");
    pub const TYPE_FILTER: (&str, &str) = ("type", "Filter");
    pub const HELP_SCROLL: (&str, &str) = ("j/k / ↑↓", "Scroll");
    pub const HELP_CLOSE: (&str, &str) = ("?/Esc", "Close");

    // SQL Modal
    pub const SQL_RUN: (&str, &str) = ("⌥Enter", "Run");
    pub const SQL_MOVE: (&str, &str) = ("↑↓←→", "Move");

    // Connection Setup
    pub const SAVE: (&str, &str) = ("^S", "Save");
    pub const TAB_NEXT: (&str, &str) = ("Tab", "Next");
    pub const TAB_PREV: (&str, &str) = ("⇧Tab", "Prev");

    // Connection Error
    pub const EDIT: (&str, &str) = ("e", "Edit");
    pub const DETAILS: (&str, &str) = ("d", "Details");
    pub const COPY: (&str, &str) = ("c", "Copy");
}

// =============================================================================
// Global Keys (Normal mode)
// =============================================================================

pub const GLOBAL_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "q",
        description: "Quit application",
        action: Action::Quit,
    },
    KeyBinding {
        key: "?",
        description: "Toggle help",
        action: Action::OpenHelp,
    },
    KeyBinding {
        key: "Ctrl+P",
        description: "Open Table Picker",
        action: Action::OpenTablePicker,
    },
    KeyBinding {
        key: "Ctrl+K",
        description: "Open Command Palette",
        action: Action::OpenCommandPalette,
    },
    KeyBinding {
        key: ":",
        description: "Enter command line",
        action: Action::EnterCommandLine,
    },
    KeyBinding {
        key: "f",
        description: "Toggle Focus mode",
        action: Action::ToggleFocus,
    },
    KeyBinding {
        key: "1/2/3",
        description: "Switch pane focus",
        action: Action::None, // Placeholder, actual action depends on key
    },
    KeyBinding {
        key: "Tab/⇧Tab",
        description: "Inspector prev/next tab",
        action: Action::None,
    },
    KeyBinding {
        key: "r",
        description: "Reload metadata",
        action: Action::ReloadMetadata,
    },
    KeyBinding {
        key: "s",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
    },
    KeyBinding {
        key: "e",
        description: "Open ER Diagram",
        action: Action::ErOpenDiagram,
    },
    KeyBinding {
        key: "c",
        description: "Open connection settings",
        action: Action::OpenConnectionSetup,
    },
];

pub const NAVIGATION_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "j / ↓",
        description: "Move down / scroll",
        action: Action::None,
    },
    KeyBinding {
        key: "k / ↑",
        description: "Move up / scroll",
        action: Action::None,
    },
    KeyBinding {
        key: "g / Home",
        description: "First item / top",
        action: Action::None,
    },
    KeyBinding {
        key: "G / End",
        description: "Last item / bottom",
        action: Action::None,
    },
    KeyBinding {
        key: "h / l",
        description: "Scroll left/right",
        action: Action::None,
    },
];

// =============================================================================
// SQL Modal
// =============================================================================

pub const SQL_MODAL_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "Alt+Enter",
        description: "Execute query",
        action: Action::SqlModalSubmit,
    },
    KeyBinding {
        key: "Esc",
        description: "Close editor",
        action: Action::CloseSqlModal,
    },
    KeyBinding {
        key: "↑↓←→",
        description: "Move cursor",
        action: Action::None,
    },
    KeyBinding {
        key: "Home/End",
        description: "Line start/end",
        action: Action::None,
    },
    KeyBinding {
        key: "Tab",
        description: "Insert tab / Accept completion",
        action: Action::None,
    },
    KeyBinding {
        key: "Ctrl+Space",
        description: "Trigger completion",
        action: Action::CompletionTrigger,
    },
    KeyBinding {
        key: "Ctrl+L",
        description: "Clear editor",
        action: Action::SqlModalClear,
    },
];

// =============================================================================
// Overlays (common)
// =============================================================================

pub const OVERLAY_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "Esc",
        description: "Close overlay / Cancel",
        action: Action::None,
    },
    KeyBinding {
        key: "Enter",
        description: "Confirm selection",
        action: Action::None,
    },
];

// =============================================================================
// Command Line
// =============================================================================

pub const COMMAND_LINE_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: ":quit",
        description: "Quit application",
        action: Action::Quit,
    },
    KeyBinding {
        key: ":help",
        description: "Show help",
        action: Action::OpenHelp,
    },
    KeyBinding {
        key: ":sql",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
    },
    KeyBinding {
        key: ":erd",
        description: "Open ER Diagram",
        action: Action::ErOpenDiagram,
    },
];

// =============================================================================
// Connection Setup
// =============================================================================

pub const CONNECTION_SETUP_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "Tab/⇧Tab",
        description: "Next/Previous field",
        action: Action::None,
    },
    KeyBinding {
        key: "Ctrl+S",
        description: "Save and connect",
        action: Action::ConnectionSetupSave,
    },
    KeyBinding {
        key: "Esc",
        description: "Cancel",
        action: Action::ConnectionSetupCancel,
    },
    KeyBinding {
        key: "Enter",
        description: "Toggle dropdown (SSL field)",
        action: Action::ConnectionSetupToggleDropdown,
    },
    KeyBinding {
        key: "↑↓",
        description: "Dropdown navigation",
        action: Action::None,
    },
];

// =============================================================================
// Connection Error
// =============================================================================

pub const CONNECTION_ERROR_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "e",
        description: "Edit connection settings",
        action: Action::ReenterConnectionSetup,
    },
    KeyBinding {
        key: "d",
        description: "Toggle error details",
        action: Action::ToggleConnectionErrorDetails,
    },
    KeyBinding {
        key: "c",
        description: "Copy error to clipboard",
        action: Action::CopyConnectionError,
    },
    KeyBinding {
        key: "j/k",
        description: "Scroll error",
        action: Action::None,
    },
    KeyBinding {
        key: "Esc",
        description: "Close",
        action: Action::CloseConnectionError,
    },
    KeyBinding {
        key: "q",
        description: "Quit",
        action: Action::Quit,
    },
];

// =============================================================================
// Confirm Dialog
// =============================================================================

pub const CONFIRM_DIALOG_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "Enter / y",
        description: "Confirm",
        action: Action::ConfirmDialogConfirm,
    },
    KeyBinding {
        key: "Esc / n",
        description: "Cancel",
        action: Action::ConfirmDialogCancel,
    },
];

// =============================================================================
// Table Picker
// =============================================================================

pub const TABLE_PICKER_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "Enter",
        description: "Select table",
        action: Action::ConfirmSelection,
    },
    KeyBinding {
        key: "↑↓",
        description: "Navigate",
        action: Action::None,
    },
    KeyBinding {
        key: "Esc",
        description: "Close",
        action: Action::CloseTablePicker,
    },
];

// =============================================================================
// Command Palette
// =============================================================================

pub const COMMAND_PALETTE_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "Enter",
        description: "Execute command",
        action: Action::None,
    },
    KeyBinding {
        key: "j/k / ↑↓",
        description: "Navigate",
        action: Action::None,
    },
    KeyBinding {
        key: "Esc",
        description: "Close",
        action: Action::CloseCommandPalette,
    },
];

// =============================================================================
// Help
// =============================================================================

pub const HELP_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key: "j / k",
        description: "Scroll down / up",
        action: Action::HelpScrollDown,
    },
    KeyBinding {
        key: "? / Esc",
        description: "Close help",
        action: Action::CloseHelp,
    },
    KeyBinding {
        key: "q",
        description: "Quit",
        action: Action::Quit,
    },
];

// =============================================================================
// Help Overlay Layout
// =============================================================================

/// Total lines in help overlay content (8 sections + 7 blank lines + key entries)
pub const HELP_TOTAL_LINES: usize = 8 + 7
    + GLOBAL_KEYS.len()
    + NAVIGATION_KEYS.len()
    + SQL_MODAL_KEYS.len()
    + OVERLAY_KEYS.len()
    + COMMAND_LINE_KEYS.len()
    + CONNECTION_SETUP_KEYS.len()
    + CONNECTION_ERROR_KEYS.len()
    + CONFIRM_DIALOG_KEYS.len();
