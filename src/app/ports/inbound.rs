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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(0b001);
    pub const ALT: Self = Self(0b010);
    pub const SHIFT: Self = Self(0b100);
    pub const CTRL_ALT: Self = Self(Self::CTRL.0 | Self::ALT.0);
    pub const CTRL_SHIFT: Self = Self(Self::CTRL.0 | Self::SHIFT.0);

    pub const fn empty() -> Self {
        Self::NONE
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    pub fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        } else {
            self.0 &= !flag.0;
        }
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
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
