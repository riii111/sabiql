#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefix {
    G,
    Z,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeySequenceState {
    #[default]
    Idle,
    WaitingSecondKey(Prefix),
}

impl KeySequenceState {
    pub fn pending_prefix(self) -> Option<Prefix> {
        match self {
            Self::WaitingSecondKey(p) => Some(p),
            Self::Idle => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_by_default() {
        let state = KeySequenceState::default();
        assert_eq!(state, KeySequenceState::Idle);
    }

    #[test]
    fn pending_prefix_returns_some_for_waiting() {
        let state = KeySequenceState::WaitingSecondKey(Prefix::Z);
        assert_eq!(state.pending_prefix(), Some(Prefix::Z));
    }

    #[test]
    fn g_prefix_is_reported_as_pending() {
        let state = KeySequenceState::WaitingSecondKey(Prefix::G);
        assert_eq!(state.pending_prefix(), Some(Prefix::G));
    }

    #[test]
    fn pending_prefix_returns_none_for_idle() {
        let state = KeySequenceState::Idle;
        assert_eq!(state.pending_prefix(), None);
    }
}
