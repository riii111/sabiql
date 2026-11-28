// State transitions:
// NotConnected → Connecting → Connected
//                           → Failed → NotConnected
// Connected → NotConnected (re-entering connection setup)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    NotConnected,
    Connecting,
    Connected,
    Failed,
}

impl ConnectionState {
    pub fn is_not_connected(self) -> bool {
        matches!(self, Self::NotConnected)
    }

    pub fn is_connecting(self) -> bool {
        matches!(self, Self::Connecting)
    }

    pub fn is_connected(self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(ConnectionState::NotConnected, true, false, false)]
    #[case(ConnectionState::Connecting, false, true, false)]
    #[case(ConnectionState::Connected, false, false, true)]
    #[case(ConnectionState::Failed, false, false, false)]
    fn predicates_match_expected_flags(
        #[case] state: ConnectionState,
        #[case] is_not_connected: bool,
        #[case] is_connecting: bool,
        #[case] is_connected: bool,
    ) {
        assert_eq!(state.is_not_connected(), is_not_connected);
        assert_eq!(state.is_connecting(), is_connecting);
        assert_eq!(state.is_connected(), is_connected);
    }
}
