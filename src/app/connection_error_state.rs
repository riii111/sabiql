use std::time::{Duration, Instant};

use super::connection_error::ConnectionErrorInfo;

#[derive(Debug, Clone, Default)]
pub struct ConnectionErrorState {
    pub error_info: Option<ConnectionErrorInfo>,
    pub details_expanded: bool,
    pub scroll_offset: usize,
    copied_feedback_expires: Option<Instant>,
}

impl ConnectionErrorState {
    const FEEDBACK_TIMEOUT_SECS: u64 = 3;

    pub fn set_error(&mut self, info: ConnectionErrorInfo) {
        self.error_info = Some(info);
        self.details_expanded = false;
        self.scroll_offset = 0;
        self.copied_feedback_expires = None;
    }

    pub fn clear(&mut self) {
        self.error_info = None;
        self.details_expanded = false;
        self.scroll_offset = 0;
        self.copied_feedback_expires = None;
    }

    pub fn toggle_details(&mut self) {
        self.details_expanded = !self.details_expanded;
        if !self.details_expanded {
            self.scroll_offset = 0;
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max_scroll: usize) {
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub fn mark_copied_at(&mut self, now: Instant) {
        self.copied_feedback_expires =
            Some(now + Duration::from_secs(Self::FEEDBACK_TIMEOUT_SECS));
    }

    pub fn is_copied_visible_at(&self, now: Instant) -> bool {
        self.copied_feedback_expires
            .is_some_and(|expires| now < expires)
    }

    pub fn clear_expired_feedback_at(&mut self, now: Instant) {
        if let Some(expires) = self.copied_feedback_expires
            && expires <= now
        {
            self.copied_feedback_expires = None;
        }
    }

    pub fn masked_details(&self) -> Option<&str> {
        self.error_info
            .as_ref()
            .map(|info| info.masked_details.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::connection_error::ConnectionErrorKind;

    fn sample_error() -> ConnectionErrorInfo {
        ConnectionErrorInfo::with_kind(ConnectionErrorKind::Timeout, "connection timed out")
    }

    fn now() -> Instant {
        Instant::now()
    }

    mod set_error {
        use super::*;

        #[test]
        fn stores_info_and_resets_ui() {
            let mut state = ConnectionErrorState {
                details_expanded: true,
                scroll_offset: 5,
                ..Default::default()
            };

            state.set_error(sample_error());

            assert!(state.error_info.is_some());
            assert!(!state.details_expanded);
            assert_eq!(state.scroll_offset, 0);
        }
    }

    mod clear {
        use super::*;

        #[test]
        fn resets_all_fields() {
            let mut state = ConnectionErrorState::default();
            state.set_error(sample_error());
            state.details_expanded = true;
            state.scroll_offset = 3;

            state.clear();

            assert!(state.error_info.is_none());
            assert!(!state.details_expanded);
            assert_eq!(state.scroll_offset, 0);
        }
    }

    mod toggle_details {
        use super::*;

        #[test]
        fn flips_expanded_state() {
            let mut state = ConnectionErrorState::default();

            state.toggle_details();
            assert!(state.details_expanded);

            state.toggle_details();
            assert!(!state.details_expanded);
        }

        #[test]
        fn resets_scroll_on_collapse() {
            let mut state = ConnectionErrorState {
                details_expanded: true,
                scroll_offset: 5,
                ..Default::default()
            };

            state.toggle_details();

            assert_eq!(state.scroll_offset, 0);
        }
    }

    mod scroll {
        use super::*;

        #[test]
        fn up_decrements_offset() {
            let mut state = ConnectionErrorState {
                scroll_offset: 5,
                ..Default::default()
            };

            state.scroll_up();

            assert_eq!(state.scroll_offset, 4);
        }

        #[test]
        fn up_stops_at_zero() {
            let mut state = ConnectionErrorState::default();

            state.scroll_up();

            assert_eq!(state.scroll_offset, 0);
        }

        #[test]
        fn down_increments_offset() {
            let mut state = ConnectionErrorState::default();

            state.scroll_down(10);

            assert_eq!(state.scroll_offset, 1);
        }

        #[test]
        fn down_stops_at_max() {
            let mut state = ConnectionErrorState {
                scroll_offset: 10,
                ..Default::default()
            };

            state.scroll_down(10);

            assert_eq!(state.scroll_offset, 10);
        }
    }

    mod copied_feedback {
        use super::*;

        #[test]
        fn visible_before_expiry() {
            let mut state = ConnectionErrorState::default();
            let t = now();

            state.mark_copied_at(t);

            assert!(state.is_copied_visible_at(t));
            assert!(state.is_copied_visible_at(t + Duration::from_secs(2)));
        }

        #[test]
        fn hidden_after_expiry() {
            let mut state = ConnectionErrorState::default();
            let t = now();

            state.mark_copied_at(t);

            assert!(!state.is_copied_visible_at(t + Duration::from_secs(4)));
        }

        #[test]
        fn clear_expired_removes_when_expired() {
            let mut state = ConnectionErrorState::default();
            let t = now();
            state.mark_copied_at(t);

            state.clear_expired_feedback_at(t + Duration::from_secs(4));

            assert!(!state.is_copied_visible_at(t));
        }

        #[test]
        fn clear_expired_keeps_when_not_expired() {
            let mut state = ConnectionErrorState::default();
            let t = now();
            state.mark_copied_at(t);

            state.clear_expired_feedback_at(t + Duration::from_secs(1));

            assert!(state.is_copied_visible_at(t));
        }
    }

    mod masked_details {
        use super::*;

        #[test]
        fn returns_none_when_no_error() {
            let state = ConnectionErrorState::default();
            assert!(state.masked_details().is_none());
        }

        #[test]
        fn returns_masked_string_when_error_exists() {
            let mut state = ConnectionErrorState::default();
            state.set_error(sample_error());

            assert!(state.masked_details().unwrap().contains("timed out"));
        }
    }
}
