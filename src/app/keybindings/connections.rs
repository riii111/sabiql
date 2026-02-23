use super::KeyBinding;
use super::types::{Key, KeyCombo};
use crate::app::action::Action;

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
        combos: &[],
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

pub const CONNECTION_ERROR_HIDDEN: &[KeyBinding] = &[
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
        combos: &[],
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

pub const CONNECTION_SELECTOR_HIDDEN: &[KeyBinding] = &[
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
