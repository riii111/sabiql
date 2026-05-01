use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct MessageState {
    last_error: Option<String>,
    last_success: Option<String>,
    expires_at: Option<Instant>,
}

impl MessageState {
    const ERROR_TIMEOUT_SECS: u64 = 3;
    const SUCCESS_TIMEOUT_SECS: u64 = 3;

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn last_success(&self) -> Option<&str> {
        self.last_success.as_deref()
    }

    pub fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }

    pub fn set_error_at(&mut self, msg: String, now: Instant) {
        self.last_error = Some(msg);
        self.last_success = None;
        self.expires_at = Some(now + Duration::from_secs(Self::ERROR_TIMEOUT_SECS));
    }

    pub fn set_success_at(&mut self, msg: String, now: Instant) {
        self.last_success = Some(msg);
        self.last_error = None;
        self.expires_at = Some(now + Duration::from_secs(Self::SUCCESS_TIMEOUT_SECS));
    }

    pub fn clear_expired_at(&mut self, now: Instant) {
        if let Some(expires) = self.expires_at
            && expires <= now
        {
            self.last_error = None;
            self.last_success = None;
            self.expires_at = None;
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.set_error_at(msg, Instant::now());
    }

    pub fn set_success(&mut self, msg: String) {
        self.set_success_at(msg, Instant::now());
    }

    pub fn clear_expired(&mut self) {
        self.clear_expired_at(Instant::now());
    }

    pub fn clear(&mut self) {
        self.last_error = None;
        self.last_success = None;
        self.expires_at = None;
    }
}

#[cfg(test)]
impl MessageState {
    pub fn set_messages_for_test(&mut self, error: Option<String>, success: Option<String>) {
        self.last_error = error;
        self.last_success = success;
    }

    pub fn set_expires_at_for_test(&mut self, expires_at: Option<Instant>) {
        self.expires_at = expires_at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_instant() -> Instant {
        Instant::now()
    }

    #[test]
    fn set_error_clears_success_message() {
        let now = fixed_instant();
        let mut state = MessageState::default();
        state.set_success_at("Success!".to_string(), now);
        assert!(state.last_success().is_some());

        state.set_error_at("Error!".to_string(), now);

        assert_eq!(state.last_error, Some("Error!".to_string()));
        assert!(state.last_success().is_none());
    }

    #[test]
    fn set_success_clears_error_message() {
        let now = fixed_instant();
        let mut state = MessageState::default();
        state.set_error_at("Error!".to_string(), now);
        assert!(state.last_error().is_some());

        state.set_success_at("Success!".to_string(), now);

        assert_eq!(state.last_success, Some("Success!".to_string()));
        assert!(state.last_error().is_none());
    }

    #[test]
    fn set_error_sets_expiration_time() {
        let now = fixed_instant();
        let mut state = MessageState::default();
        assert!(state.expires_at.is_none());

        state.set_error_at("Error!".to_string(), now);

        assert!(state.expires_at.is_some());
        assert_eq!(
            state.expires_at,
            Some(now + Duration::from_secs(MessageState::ERROR_TIMEOUT_SECS))
        );
    }

    #[test]
    fn clear_expired_at_removes_expired_messages() {
        let now = fixed_instant();
        let mut state = MessageState {
            last_error: Some("Error".to_string()),
            expires_at: Some(now.checked_sub(Duration::from_secs(1)).unwrap()),
            ..Default::default()
        };

        state.clear_expired_at(now);

        assert!(state.last_error().is_none());
        assert!(state.expires_at.is_none());
    }

    #[test]
    fn clear_expired_at_keeps_unexpired_messages() {
        let now = fixed_instant();
        let mut state = MessageState {
            last_error: Some("Error".to_string()),
            expires_at: Some(now + Duration::from_secs(10)),
            ..Default::default()
        };

        state.clear_expired_at(now);

        assert!(state.last_error().is_some());
        assert!(state.expires_at.is_some());
    }

    #[test]
    fn clear_removes_all_messages() {
        let now = fixed_instant();
        let mut state = MessageState::default();
        state.set_error_at("Error".to_string(), now);

        state.clear();

        assert!(state.last_error().is_none());
        assert!(state.last_success().is_none());
        assert!(state.expires_at.is_none());
    }
}
