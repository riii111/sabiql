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
        desc_short: "Field",
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
        description: "Toggle dropdown",
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
