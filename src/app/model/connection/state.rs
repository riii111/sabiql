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
