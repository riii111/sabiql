use super::KeyBinding;

// =============================================================================
// Overlays (common display hints)
// =============================================================================

pub mod overlay {
    use crate::update::action::Action;
    use crate::update::input::keybindings::KeyBinding;

    pub const ESC_CANCEL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Close overlay / Cancel",
        action: Action::None,
        combos: &[],
    };

    pub const ESC_CLOSE: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close overlay",
        action: Action::None,
        combos: &[],
    };

    pub const ENTER_EXECUTE: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
        combos: &[],
    };

    pub const ENTER_SELECT: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Confirm selection",
        action: Action::None,
        combos: &[],
    };

    pub const NAVIGATE_JK: KeyBinding = KeyBinding {
        key_short: "^N/^P/j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate items",
        action: Action::None,
        combos: &[],
    };

    pub const TYPE_FILTER: KeyBinding = KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
        combos: &[],
    };

    pub const ERROR_OPEN: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Error",
        description: "View error details",
        action: Action::None,
        combos: &[],
    };
}

pub const OVERLAY_KEYS: &[KeyBinding] = &[
    overlay::ESC_CANCEL,
    overlay::ESC_CLOSE,
    overlay::ENTER_EXECUTE,
    overlay::ENTER_SELECT,
    overlay::NAVIGATE_JK,
    overlay::TYPE_FILTER,
    overlay::ERROR_OPEN,
];
