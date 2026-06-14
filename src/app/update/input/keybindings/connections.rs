use super::ModeRow;
use crate::model::shared::settings::KeymapPreset;
use crate::update::input::keybindings::KeyBinding;

// =============================================================================
// Connection Setup
// =============================================================================

pub mod connection_setup {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const TAB_NAV: KeyBinding = KeyBinding {
        key_short: "Tab/⇧Tab",
        key: "Tab/⇧Tab",
        desc_short: "Next/Prev",
        description: "Next/Previous field",
        action: Action::None,
        combos: &[],
    };

    pub const TAB_NEXT: KeyBinding = KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Next",
        description: "Next field",
        action: Action::None,
        combos: &[],
    };

    pub const TAB_PREV: KeyBinding = KeyBinding {
        key_short: "⇧Tab",
        key: "⇧Tab",
        desc_short: "Prev",
        description: "Previous field",
        action: Action::None,
        combos: &[],
    };

    pub const SAVE: KeyBinding = KeyBinding {
        key_short: "^S",
        key: "Ctrl+S",
        desc_short: "Connect",
        description: "Save and connect",
        action: Action::ConnectionSetupSave,
        combos: &[KeyCombo::ctrl(Key::Char('s'))],
    };

    pub const SAVE_IDE: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Connect",
        description: "Save and connect",
        action: Action::ConnectionSetupSave,
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const ESC_CANCEL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        action: Action::ConnectionSetupCancel,
        combos: &[KeyCombo::plain(Key::Esc)],
    };

    pub const ENTER_DROPDOWN: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Toggle",
        description: "Toggle dropdown (SSL field)",
        action: Action::ConnectionSetupToggleDropdown,
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const DROPDOWN_NAV: KeyBinding = KeyBinding {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Select",
        description: "Dropdown navigation",
        action: Action::None,
        combos: &[],
    };
}

pub const CONNECTION_SETUP_KEYS: &[KeyBinding] = &[
    connection_setup::TAB_NAV,
    connection_setup::TAB_NEXT,
    connection_setup::TAB_PREV,
    connection_setup::SAVE,
    connection_setup::ESC_CANCEL,
    connection_setup::ENTER_DROPDOWN,
    connection_setup::DROPDOWN_NAV,
];

pub fn connection_setup_save(preset: KeymapPreset) -> &'static KeyBinding {
    match preset {
        KeymapPreset::Default => &connection_setup::SAVE,
        KeymapPreset::Ide => &connection_setup::SAVE_IDE,
    }
}

// =============================================================================
// Connection Error
// =============================================================================

pub mod connection_error {
    use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const EDIT: ModeRow = ModeRow {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection settings",
        bindings: &[ExecBinding {
            action: Action::ReenterConnectionSetup,
            combos: &[KeyCombo::plain(Key::Char('e'))],
        }],
    };

    pub const SWITCH: ModeRow = ModeRow {
        key_short: "s",
        key: "s",
        desc_short: "Switch",
        description: "Switch to another connection",
        bindings: &[ExecBinding {
            action: Action::OpenModal(ModalKind::ConnectionSelector),
            combos: &[KeyCombo::plain(Key::Char('s'))],
        }],
    };

    pub const DETAILS: ModeRow = ModeRow {
        key_short: "d",
        key: "d",
        desc_short: "Details",
        description: "Toggle error details",
        bindings: &[ExecBinding {
            action: Action::ToggleConnectionErrorDetails,
            combos: &[KeyCombo::plain(Key::Char('d'))],
        }],
    };

    pub const COPY: ModeRow = ModeRow {
        key_short: "y",
        key: "y",
        desc_short: "Copy",
        description: "Copy error to clipboard",
        bindings: &[ExecBinding {
            action: Action::CopyConnectionError,
            combos: &[KeyCombo::plain(Key::Char('y'))],
        }],
    };

    pub const SCROLL: ModeRow = ModeRow {
        key_short: "^N/^P/j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll error",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::ConnectionError,
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
                    target: ScrollTarget::ConnectionError,
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
    };

    pub const ESC_CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseConnectionError,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };

    pub const RETRY: ModeRow = ModeRow {
        key_short: "r",
        key: "r",
        desc_short: "Retry",
        description: "Retry service connection",
        bindings: &[ExecBinding {
            action: Action::RetryServiceConnection,
            combos: &[KeyCombo::plain(Key::Char('r'))],
        }],
    };
}

pub const CONNECTION_ERROR_ROWS: &[ModeRow] = &[
    connection_error::EDIT,
    connection_error::SWITCH,
    connection_error::DETAILS,
    connection_error::COPY,
    connection_error::SCROLL,
    connection_error::ESC_CLOSE,
    connection_error::RETRY,
];

// =============================================================================
// Connection Selector
// =============================================================================

pub mod connection_selector {
    use crate::update::action::{Action, ListMotion, ListTarget, ModalKind};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const CONFIRM: ModeRow = ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm selection",
        bindings: &[ExecBinding {
            action: Action::ConfirmConnectionSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    };

    pub const SELECT: ModeRow = ModeRow {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓ / j / k",
        desc_short: "Nav",
        description: "Select connection",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::ConnectionList,
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
                    target: ListTarget::ConnectionList,
                    motion: ListMotion::Previous,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::plain(Key::Up),
                    KeyCombo::ctrl(Key::Char('p')),
                ],
            },
        ],
    };

    pub const NEW: ModeRow = ModeRow {
        key_short: "n",
        key: "n",
        desc_short: "New",
        description: "New connection",
        bindings: &[ExecBinding {
            action: Action::OpenModal(ModalKind::ConnectionSetup),
            combos: &[KeyCombo::plain(Key::Char('n'))],
        }],
    };

    pub const EDIT: ModeRow = ModeRow {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit connection",
        bindings: &[ExecBinding {
            action: Action::RequestEditSelectedConnection,
            combos: &[KeyCombo::plain(Key::Char('e'))],
        }],
    };

    pub const DELETE: ModeRow = ModeRow {
        key_short: "d",
        key: "d",
        desc_short: "Delete",
        description: "Delete connection",
        bindings: &[ExecBinding {
            action: Action::RequestDeleteSelectedConnection,
            combos: &[KeyCombo::plain(Key::Char('d'))],
        }],
    };

    pub const CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close selector",
        bindings: &[ExecBinding {
            action: Action::Escape,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };
}

pub const CONNECTION_SELECTOR_ROWS: &[ModeRow] = &[
    connection_selector::CONFIRM,
    connection_selector::SELECT,
    connection_selector::NEW,
    connection_selector::EDIT,
    connection_selector::DELETE,
    connection_selector::CLOSE,
];
