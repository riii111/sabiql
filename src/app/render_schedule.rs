//! Pure functions for calculating animation deadlines.
//!
//! These functions are I/O-free and deterministic, suitable for use in the app layer.
//! The UI layer uses the returned deadlines to schedule wake-ups.

use std::time::{Duration, Instant};

use crate::app::er_state::ErStatus;
use crate::app::input_mode::InputMode;
use crate::app::query_execution::QueryStatus;
use crate::app::state::AppState;

/// Interval for spinner animation updates (150ms for smooth animation at ~6.7 FPS)
const SPINNER_INTERVAL: Duration = Duration::from_millis(150);

/// Interval for cursor blink updates (500ms for standard blink rate)
const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);

/// Calculates the next animation deadline based on the current state.
///
/// Returns `Some(Instant)` when an animation is active and needs a timed update.
/// Returns `None` when no animations are active (caller can wait indefinitely for input).
///
/// # Animation sources (in priority order):
/// 1. Spinner: Query execution or ER diagram preparation
/// 2. Message timeout: Error/success messages with expiration
/// 3. Result highlight: Temporary border highlight after query completion
/// 4. Cursor blink: Active in modal input modes (SqlModal, TablePicker, CommandLine)
pub fn next_animation_deadline(state: &AppState, now: Instant) -> Option<Instant> {
    let mut earliest: Option<Instant> = None;

    if has_active_spinner(state) {
        earliest = min_instant(earliest, Some(now + SPINNER_INTERVAL));
    }

    if let Some(expires_at) = state.messages.expires_at {
        earliest = min_instant(earliest, Some(expires_at));
    }

    if let Some(highlight_until) = state.query.result_highlight_until {
        earliest = min_instant(earliest, Some(highlight_until));
    }

    // Cursor blink is the slowest animation; skip if faster ones are active
    if has_blinking_cursor(state) && earliest.is_none() {
        earliest = Some(now + CURSOR_BLINK_INTERVAL);
    }

    earliest
}

/// Returns true if a spinner animation is currently active.
fn has_active_spinner(state: &AppState) -> bool {
    state.query.status == QueryStatus::Running
        || state.er_preparation.status == ErStatus::Waiting
}

/// Returns true if the current input mode has a blinking cursor.
fn has_blinking_cursor(state: &AppState) -> bool {
    matches!(
        state.ui.input_mode,
        InputMode::SqlModal | InputMode::TablePicker | InputMode::CommandLine
    )
}

/// Returns the earlier of two optional instants.
fn min_instant(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> AppState {
        AppState::new("test".to_string())
    }

    mod next_animation_deadline_tests {
        use super::*;

        #[test]
        fn idle_state_returns_none() {
            let state = create_test_state();
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_none());
        }

        #[test]
        fn query_running_returns_spinner_interval() {
            let mut state = create_test_state();
            state.query.status = QueryStatus::Running;
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + SPINNER_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn er_waiting_returns_spinner_interval() {
            let mut state = create_test_state();
            state.er_preparation.status = ErStatus::Waiting;
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + SPINNER_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn message_timeout_returns_expiration() {
            let mut state = create_test_state();
            let now = Instant::now();
            let expires_at = now + Duration::from_secs(2);
            state.messages.expires_at = Some(expires_at);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(expires_at));
        }

        #[test]
        fn result_highlight_returns_expiration() {
            let mut state = create_test_state();
            let now = Instant::now();
            let highlight_until = now + Duration::from_millis(500);
            state.query.result_highlight_until = Some(highlight_until);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(highlight_until));
        }

        #[test]
        fn sql_modal_returns_cursor_blink_interval() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::SqlModal;
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + CURSOR_BLINK_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn table_picker_returns_cursor_blink_interval() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::TablePicker;
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + CURSOR_BLINK_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn command_line_returns_cursor_blink_interval() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::CommandLine;
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + CURSOR_BLINK_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn spinner_takes_priority_over_cursor_blink() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::SqlModal;
            state.query.status = QueryStatus::Running;
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            // Spinner interval (150ms) is shorter than cursor blink (500ms)
            let expected = now + SPINNER_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn earlier_message_timeout_takes_priority() {
            let mut state = create_test_state();
            state.query.status = QueryStatus::Running;
            let now = Instant::now();
            // Message expires before spinner would update
            let expires_at = now + Duration::from_millis(50);
            state.messages.expires_at = Some(expires_at);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(expires_at));
        }

        #[test]
        fn multiple_animations_returns_earliest() {
            let mut state = create_test_state();
            let now = Instant::now();

            // Set up multiple animation sources
            state.query.status = QueryStatus::Running; // 150ms
            state.messages.expires_at = Some(now + Duration::from_secs(2)); // 2000ms
            state.query.result_highlight_until = Some(now + Duration::from_millis(100)); // 100ms

            let deadline = next_animation_deadline(&state, now);

            // Result highlight (100ms) is earliest
            assert_eq!(deadline, Some(now + Duration::from_millis(100)));
        }
    }

    mod has_active_spinner_tests {
        use super::*;

        #[test]
        fn idle_query_returns_false() {
            let state = create_test_state();

            assert!(!has_active_spinner(&state));
        }

        #[test]
        fn running_query_returns_true() {
            let mut state = create_test_state();
            state.query.status = QueryStatus::Running;

            assert!(has_active_spinner(&state));
        }

        #[test]
        fn er_idle_returns_false() {
            let state = create_test_state();

            assert!(!has_active_spinner(&state));
        }

        #[test]
        fn er_waiting_returns_true() {
            let mut state = create_test_state();
            state.er_preparation.status = ErStatus::Waiting;

            assert!(has_active_spinner(&state));
        }

        #[test]
        fn er_rendering_returns_false() {
            let mut state = create_test_state();
            state.er_preparation.status = ErStatus::Rendering;

            assert!(!has_active_spinner(&state));
        }
    }

    mod has_blinking_cursor_tests {
        use super::*;

        #[test]
        fn normal_mode_returns_false() {
            let state = create_test_state();

            assert!(!has_blinking_cursor(&state));
        }

        #[test]
        fn sql_modal_returns_true() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::SqlModal;

            assert!(has_blinking_cursor(&state));
        }

        #[test]
        fn table_picker_returns_true() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::TablePicker;

            assert!(has_blinking_cursor(&state));
        }

        #[test]
        fn command_line_returns_true() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::CommandLine;

            assert!(has_blinking_cursor(&state));
        }

        #[test]
        fn help_mode_returns_false() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::Help;

            assert!(!has_blinking_cursor(&state));
        }
    }

    mod min_instant_tests {
        use super::*;

        #[test]
        fn both_none_returns_none() {
            assert!(min_instant(None, None).is_none());
        }

        #[test]
        fn first_some_returns_first() {
            let now = Instant::now();
            assert_eq!(min_instant(Some(now), None), Some(now));
        }

        #[test]
        fn second_some_returns_second() {
            let now = Instant::now();
            assert_eq!(min_instant(None, Some(now)), Some(now));
        }

        #[test]
        fn both_some_returns_earlier() {
            let now = Instant::now();
            let later = now + Duration::from_secs(1);

            assert_eq!(min_instant(Some(now), Some(later)), Some(now));
            assert_eq!(min_instant(Some(later), Some(now)), Some(now));
        }
    }
}
