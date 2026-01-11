/// Connection lifecycle state for lazy connection management.
///
/// State transitions:
/// - NotConnected → Connecting (when entering Main screen with dsn)
/// - Connecting → Connected (on successful metadata fetch)
/// - Connecting → Failed (on connection error)
/// - Failed → NotConnected (when re-entering connection setup)
/// - Connected → NotConnected (when re-entering connection setup)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    NotConnected,
    Connecting,
    Connected,
    Failed,
}

impl ConnectionState {
    pub fn is_not_connected(&self) -> bool {
        matches!(self, Self::NotConnected)
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self, Self::Connecting)
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn default_returns_not_connected() {
        let state = ConnectionState::default();
        assert!(state.is_not_connected());
    }

    #[rstest]
    #[case(ConnectionState::NotConnected, true, false, false, false)]
    #[case(ConnectionState::Connecting, false, true, false, false)]
    #[case(ConnectionState::Connected, false, false, true, false)]
    #[case(ConnectionState::Failed, false, false, false, true)]
    fn predicate_returns_expected(
        #[case] state: ConnectionState,
        #[case] is_not_connected: bool,
        #[case] is_connecting: bool,
        #[case] is_connected: bool,
        #[case] is_failed: bool,
    ) {
        assert_eq!(state.is_not_connected(), is_not_connected);
        assert_eq!(state.is_connecting(), is_connecting);
        assert_eq!(state.is_connected(), is_connected);
        assert_eq!(state.is_failed(), is_failed);
    }
}
