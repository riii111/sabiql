use super::types::{Key, KeyCombo};
use super::{ExecBinding, KeyBinding, ModeRow};
use crate::app::action::Action;

// =============================================================================
// Connection Setup
// =============================================================================

pub const CONNECTION_SETUP_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "Tab/⇧Tab",
        key: "Tab/⇧Tab",
        desc_short: "Next/Prev",
        description: "Next/Previous field",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Next",
        description: "Next field",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "⇧Tab",
        key: "⇧Tab",
        desc_short: "Prev",
        description: "Previous field",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "^S",
        key: "Ctrl+S",
        desc_short: "Connect",
        description: "Save and connect",
        action: Action::ConnectionSetupSave,
        combos: &[KeyCombo::ctrl(Key::Char('s'))],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        action: Action::ConnectionSetupCancel,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Toggle",
        description: "Toggle dropdown (SSL field)",
        action: Action::ConnectionSetupToggleDropdown,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
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

pub const CONNECTION_ERROR_ROWS: &[ModeRow] = &[
    ModeRow {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection settings",
        bindings: &[ExecBinding {
            action: Action::ReenterConnectionSetup,
            combos: &[KeyCombo::plain(Key::Char('e'))],
        }],
    },
    ModeRow {
        key_short: "s",
        key: "s",
        desc_short: "Switch",
        description: "Switch to another connection",
        bindings: &[ExecBinding {
            action: Action::OpenConnectionSelector,
            combos: &[KeyCombo::plain(Key::Char('s'))],
        }],
    },
    ModeRow {
        key_short: "d",
        key: "d",
        desc_short: "Details",
        description: "Toggle error details",
        bindings: &[ExecBinding {
            action: Action::ToggleConnectionErrorDetails,
            combos: &[KeyCombo::plain(Key::Char('d'))],
        }],
    },
    ModeRow {
        key_short: "c",
        key: "c",
        desc_short: "Copy",
        description: "Copy error to clipboard",
        bindings: &[ExecBinding {
            action: Action::CopyConnectionError,
            combos: &[KeyCombo::plain(Key::Char('c'))],
        }],
    },
    ModeRow {
        key_short: "j/k",
        key: "j/k",
        desc_short: "Scroll",
        description: "Scroll error",
        bindings: &[
            ExecBinding {
                action: Action::ScrollConnectionErrorDown,
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::ScrollConnectionErrorUp,
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
            action: Action::CloseConnectionError,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    },
    ModeRow {
        key_short: "r",
        key: "r",
        desc_short: "Retry",
        description: "Retry service connection",
        bindings: &[ExecBinding {
            action: Action::RetryServiceConnection,
            combos: &[KeyCombo::plain(Key::Char('r'))],
        }],
    },
];

// =============================================================================
// Connection Selector
// =============================================================================

pub const CONNECTION_SELECTOR_ROWS: &[ModeRow] = &[
    ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm selection",
        bindings: &[ExecBinding {
            action: Action::ConfirmConnectionSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    },
    ModeRow {
        key_short: "↑/↓",
        key: "↑ / ↓ / j / k",
        desc_short: "Select",
        description: "Select connection",
        bindings: &[
            ExecBinding {
                action: Action::ConnectionListSelectNext,
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::ConnectionListSelectPrevious,
                combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
            },
        ],
    },
    ModeRow {
        key_short: "n",
        key: "n",
        desc_short: "New",
        description: "New connection",
        bindings: &[ExecBinding {
            action: Action::OpenConnectionSetup,
            combos: &[KeyCombo::plain(Key::Char('n'))],
        }],
    },
    ModeRow {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection",
        bindings: &[ExecBinding {
            action: Action::RequestEditSelectedConnection,
            combos: &[KeyCombo::plain(Key::Char('e'))],
        }],
    },
    ModeRow {
        key_short: "d",
        key: "d",
        desc_short: "Delete",
        description: "Delete connection",
        bindings: &[ExecBinding {
            action: Action::RequestDeleteSelectedConnection,
            combos: &[KeyCombo::plain(Key::Char('d'))],
        }],
    },
    ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close selector",
        bindings: &[ExecBinding {
            action: Action::Escape,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    },
];
