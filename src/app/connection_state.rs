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
    /// Returns true if connection has not been attempted yet
    pub fn is_not_connected(&self) -> bool {
        matches!(self, Self::NotConnected)
    }

    /// Returns true if connection is in progress
    pub fn is_connecting(&self) -> bool {
        matches!(self, Self::Connecting)
    }

    /// Returns true if successfully connected
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Returns true if connection failed
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_not_connected() {
        let state = ConnectionState::default();
        assert!(state.is_not_connected());
    }

    #[test]
    fn state_predicates_work_correctly() {
        assert!(ConnectionState::NotConnected.is_not_connected());
        assert!(!ConnectionState::NotConnected.is_connecting());
        assert!(!ConnectionState::NotConnected.is_connected());
        assert!(!ConnectionState::NotConnected.is_failed());

        assert!(!ConnectionState::Connecting.is_not_connected());
        assert!(ConnectionState::Connecting.is_connecting());
        assert!(!ConnectionState::Connecting.is_connected());
        assert!(!ConnectionState::Connecting.is_failed());

        assert!(!ConnectionState::Connected.is_not_connected());
        assert!(!ConnectionState::Connected.is_connecting());
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Connected.is_failed());

        assert!(!ConnectionState::Failed.is_not_connected());
        assert!(!ConnectionState::Failed.is_connecting());
        assert!(!ConnectionState::Failed.is_connected());
        assert!(ConnectionState::Failed.is_failed());
    }
}
