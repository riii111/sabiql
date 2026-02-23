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
    /// Exhaustive-match catch-alls for key_translator; never matched in handlers.
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
