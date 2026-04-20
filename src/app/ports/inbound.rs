use bitflags::bitflags;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Init,
    Key(KeyCombo),
    Paste(String),
    Resize(u16, u16),
}

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

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers: u8 {
        const CTRL = 0b001;
        const ALT = 0b010;
        const SHIFT = 0b100;
    }
}

impl Modifiers {
    pub const NONE: Self = Self::empty();
    pub const CTRL_ALT: Self = Self::from_bits_retain(Self::CTRL.bits() | Self::ALT.bits());
    pub const CTRL_SHIFT: Self = Self::from_bits_retain(Self::CTRL.bits() | Self::SHIFT.bits());
}

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
    pub const fn ctrl_alt(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::CTRL_ALT,
        }
    }
    pub const fn ctrl_shift(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::CTRL_SHIFT,
        }
    }
    pub const fn shift(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::SHIFT,
        }
    }
}
