use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct MessageState {
    pub last_error: Option<String>,
    pub last_success: Option<String>,
    pub expires_at: Option<Instant>,
}

impl MessageState {
    const TIMEOUT_SECS: u64 = 3;

    pub fn set_error(&mut self, msg: String) {
        self.last_error = Some(msg);
        self.last_success = None;
        self.expires_at = Some(Instant::now() + Duration::from_secs(Self::TIMEOUT_SECS));
    }

    pub fn set_success(&mut self, msg: String) {
        self.last_success = Some(msg);
        self.last_error = None;
        self.expires_at = Some(Instant::now() + Duration::from_secs(Self::TIMEOUT_SECS));
    }

    pub fn clear_expired(&mut self) {
        if let Some(expires) = self.expires_at
            && expires <= Instant::now()
        {
            self.last_error = None;
            self.last_success = None;
            self.expires_at = None;
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.last_error = None;
        self.last_success = None;
        self.expires_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_error_clears_success_message() {
        let mut state = MessageState::default();
        state.set_success("Success!".to_string());
        assert!(state.last_success.is_some());

        state.set_error("Error!".to_string());

        assert_eq!(state.last_error, Some("Error!".to_string()));
        assert!(state.last_success.is_none());
    }

    #[test]
    fn set_success_clears_error_message() {
        let mut state = MessageState::default();
        state.set_error("Error!".to_string());
        assert!(state.last_error.is_some());

        state.set_success("Success!".to_string());

        assert_eq!(state.last_success, Some("Success!".to_string()));
        assert!(state.last_error.is_none());
    }

    #[test]
    fn set_error_sets_expiration_time() {
        let mut state = MessageState::default();
        assert!(state.expires_at.is_none());

        state.set_error("Error!".to_string());

        assert!(state.expires_at.is_some());
    }

    #[test]
    fn clear_expired_removes_expired_messages() {
        let mut state = MessageState::default();
        state.last_error = Some("Error".to_string());
        state.expires_at = Some(Instant::now() - Duration::from_secs(1));

        state.clear_expired();

        assert!(state.last_error.is_none());
        assert!(state.expires_at.is_none());
    }

    #[test]
    fn clear_expired_keeps_unexpired_messages() {
        let mut state = MessageState::default();
        state.last_error = Some("Error".to_string());
        state.expires_at = Some(Instant::now() + Duration::from_secs(10));

        state.clear_expired();

        assert!(state.last_error.is_some());
        assert!(state.expires_at.is_some());
    }

    #[test]
    fn clear_removes_all_messages() {
        let mut state = MessageState::default();
        state.set_error("Error".to_string());

        state.clear();

        assert!(state.last_error.is_none());
        assert!(state.last_success.is_none());
        assert!(state.expires_at.is_none());
    }
}
