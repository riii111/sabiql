use std::time::{Duration, Instant};

use crate::model::app_state::AppState;
use crate::model::er_state::ErStatus;
use crate::model::shared::input_mode::InputMode;

const SPINNER_INTERVAL: Duration = Duration::from_millis(150);

const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);

pub fn next_animation_deadline(state: &AppState, now: Instant) -> Option<Instant> {
    let mut earliest = [
        has_active_spinner(state).then(|| now + SPINNER_INTERVAL),
        state.transient_feedback_deadline(),
        state.sql_modal.completion_debounce(),
        state.motion.next_frame(now),
    ]
    .into_iter()
    .flatten()
    .min();

    // Cursor blink is the slowest; skip if faster timers are active
    if has_blinking_cursor(state) && earliest.is_none() {
        earliest = Some(now + CURSOR_BLINK_INTERVAL);
    }

    earliest
}

fn has_active_spinner(state: &AppState) -> bool {
    state.query.is_running() || state.er_preparation.status() == ErStatus::Waiting
}

fn has_blinking_cursor(state: &AppState) -> bool {
    matches!(
        state.input_mode(),
        InputMode::SqlModal
            | InputMode::TablePicker
            | InputMode::CommandLine
            | InputMode::CellEdit
            | InputMode::ConnectionSetup
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn create_test_state() -> AppState {
        AppState::new("test".to_string())
    }

    mod next_animation_deadline_tests {
        use super::*;

        const MESSAGE_TIMEOUT: Duration = Duration::from_secs(3);

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
            let now = Instant::now();
            let _ = state.query.begin_running(now);

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + SPINNER_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn er_waiting_returns_spinner_interval() {
            let mut state = create_test_state();
            let _ = state.er_preparation.start_waiting_run();
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + SPINNER_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn success_message_timeout_returns_expiration() {
            let mut state = create_test_state();
            let now = Instant::now();
            let expires_at = now + Duration::from_secs(2);
            state.messages.set_success_at(
                "message".to_string(),
                expires_at.checked_sub(MESSAGE_TIMEOUT).unwrap(),
            );

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(expires_at));
        }

        #[test]
        fn result_highlight_returns_expiration() {
            let mut state = create_test_state();
            let now = Instant::now();
            let highlight_until = now + Duration::from_millis(500);
            state.query.set_result_highlight(highlight_until);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(highlight_until));
        }

        #[test]
        fn connection_error_copied_feedback_returns_expiration() {
            let mut state = create_test_state();
            let now = Instant::now();
            state.connection_error.mark_copied_at(now);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(now + Duration::from_secs(3)));
        }

        #[test]
        fn earlier_deadline_takes_priority_over_connection_error_copied_feedback() {
            let mut state = create_test_state();
            let now = Instant::now();
            let highlight_until = now + Duration::from_millis(100);
            state.connection_error.mark_copied_at(now);
            state.query.set_result_highlight(highlight_until);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(highlight_until));
        }

        #[rstest]
        #[case(InputMode::SqlModal)]
        #[case(InputMode::TablePicker)]
        #[case(InputMode::CommandLine)]
        #[case(InputMode::CellEdit)]
        #[case(InputMode::ConnectionSetup)]
        fn editing_modes_schedule_cursor_blink(#[case] mode: InputMode) {
            let mut state = create_test_state();
            state.modal.set_mode(mode);
            let now = Instant::now();

            assert_eq!(
                next_animation_deadline(&state, now),
                Some(now + CURSOR_BLINK_INTERVAL)
            );
        }

        #[test]
        fn help_mode_does_not_schedule_cursor_blink() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::Help);

            assert_eq!(next_animation_deadline(&state, Instant::now()), None);
        }

        #[test]
        fn er_rendering_does_not_schedule_spinner() {
            let mut state = create_test_state();
            let _ = state.er_preparation.start_waiting_run();
            state.er_preparation.mark_rendering();

            assert_eq!(next_animation_deadline(&state, Instant::now()), None);
        }

        #[test]
        fn spinner_takes_priority_over_cursor_blink() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::SqlModal);
            let now = Instant::now();
            let _ = state.query.begin_running(now);

            let deadline = next_animation_deadline(&state, now);

            // Spinner interval (150ms) is shorter than cursor blink (500ms)
            let expected = now + SPINNER_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn earlier_success_message_timeout_takes_priority() {
            let mut state = create_test_state();
            let now = Instant::now();
            let _ = state.query.begin_running(now);
            // Message expires before spinner would update
            let expires_at = now + Duration::from_millis(50);
            state.messages.set_success_at(
                "message".to_string(),
                expires_at.checked_sub(MESSAGE_TIMEOUT).unwrap(),
            );

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(expires_at));
        }

        #[test]
        fn multiple_animations_returns_earliest() {
            let mut state = create_test_state();
            let now = Instant::now();

            let _ = state.query.begin_running(now);
            let message_expires_at = now + Duration::from_secs(2);
            state.messages.set_success_at(
                "message".to_string(),
                message_expires_at.checked_sub(MESSAGE_TIMEOUT).unwrap(),
            );
            state
                .query
                .set_result_highlight(now + Duration::from_millis(100));

            let deadline = next_animation_deadline(&state, now);

            // Result highlight (100ms) < Spinner (150ms) < Message (2000ms)
            assert_eq!(deadline, Some(now + Duration::from_millis(100)));
        }

        #[test]
        fn active_motion_schedules_its_next_frame() {
            let mut state = create_test_state();
            let now = Instant::now();
            state.motion.enable();
            state.motion.begin_modal_close(now);
            state.messages.set_success_at(
                "message".to_string(),
                (now + Duration::from_secs(2))
                    .checked_sub(MESSAGE_TIMEOUT)
                    .unwrap(),
            );

            let motion_frame = state.motion.next_frame(now);

            assert!(motion_frame.is_some());
            assert_eq!(next_animation_deadline(&state, now), motion_frame);
        }

        #[test]
        fn earlier_timer_takes_priority_over_motion_frame() {
            let mut state = create_test_state();
            let now = Instant::now();
            let highlight_until = now + Duration::from_millis(1);
            state.motion.enable();
            state.motion.begin_modal_close(now);
            state.query.set_result_highlight(highlight_until);

            assert_eq!(next_animation_deadline(&state, now), Some(highlight_until));
        }

        #[test]
        fn completion_debounce_returns_expiration() {
            let mut state = create_test_state();
            let now = Instant::now();
            let debounce_until = now + Duration::from_millis(100);
            state.sql_modal.schedule_completion(debounce_until);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(debounce_until));
        }
    }
}
