use std::time::{Duration, Instant};

use crate::app::er_state::ErStatus;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

const SPINNER_INTERVAL: Duration = Duration::from_millis(150);

const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);

pub fn next_animation_deadline(state: &AppState, now: Instant) -> Option<Instant> {
    let mut earliest: Option<Instant> = None;

    if has_active_spinner(state) {
        earliest = min_instant(earliest, Some(now + SPINNER_INTERVAL));
    }

    if let Some(expires_at) = state.messages.expires_at {
        earliest = min_instant(earliest, Some(expires_at));
    }

    if let Some(highlight_until) = state.query.result_highlight_until() {
        earliest = min_instant(earliest, Some(highlight_until));
    }

    if let Some(debounce_until) = state.sql_modal.completion_debounce {
        earliest = min_instant(earliest, Some(debounce_until));
    }

    if let Some(flash) = state.result_interaction.yank_flash {
        earliest = min_instant(earliest, Some(flash.until));
    }

    if let Some(until) = state.sql_modal.yank_flash_until {
        earliest = min_instant(earliest, Some(until));
    }

    // Cursor blink is the slowest; skip if faster timers are active
    if has_blinking_cursor(state) && earliest.is_none() {
        earliest = Some(now + CURSOR_BLINK_INTERVAL);
    }

    earliest
}

fn has_active_spinner(state: &AppState) -> bool {
    state.query.is_running() || state.er_preparation.status == ErStatus::Waiting
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

fn min_instant(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        _ => a.or(b),
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
            let now = Instant::now();
            state.query.begin_running(now);

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
            state.query.set_result_highlight(highlight_until);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(highlight_until));
        }

        #[test]
        fn sql_modal_returns_cursor_blink_interval() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::SqlModal);
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + CURSOR_BLINK_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn table_picker_returns_cursor_blink_interval() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::TablePicker);
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + CURSOR_BLINK_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn command_line_returns_cursor_blink_interval() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::CommandLine);
            let now = Instant::now();

            let deadline = next_animation_deadline(&state, now);

            assert!(deadline.is_some());
            let expected = now + CURSOR_BLINK_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn spinner_takes_priority_over_cursor_blink() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::SqlModal);
            let now = Instant::now();
            state.query.begin_running(now);

            let deadline = next_animation_deadline(&state, now);

            // Spinner interval (150ms) is shorter than cursor blink (500ms)
            let expected = now + SPINNER_INTERVAL;
            assert_eq!(deadline.unwrap(), expected);
        }

        #[test]
        fn earlier_message_timeout_takes_priority() {
            let mut state = create_test_state();
            let now = Instant::now();
            state.query.begin_running(now);
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

            state.query.begin_running(now);
            state.messages.expires_at = Some(now + Duration::from_secs(2));
            state
                .query
                .set_result_highlight(now + Duration::from_millis(100));

            let deadline = next_animation_deadline(&state, now);

            // Result highlight (100ms) < Spinner (150ms) < Message (2000ms)
            assert_eq!(deadline, Some(now + Duration::from_millis(100)));
        }

        #[test]
        fn completion_debounce_returns_expiration() {
            let mut state = create_test_state();
            let now = Instant::now();
            let debounce_until = now + Duration::from_millis(100);
            state.sql_modal.completion_debounce = Some(debounce_until);

            let deadline = next_animation_deadline(&state, now);

            assert_eq!(deadline, Some(debounce_until));
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
            state.query.begin_running(Instant::now());

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
            state.modal.set_mode(InputMode::SqlModal);

            assert!(has_blinking_cursor(&state));
        }

        #[test]
        fn table_picker_returns_true() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::TablePicker);

            assert!(has_blinking_cursor(&state));
        }

        #[test]
        fn command_line_returns_true() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::CommandLine);

            assert!(has_blinking_cursor(&state));
        }

        #[test]
        fn connection_setup_returns_true() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::ConnectionSetup);

            assert!(has_blinking_cursor(&state));
        }

        #[test]
        fn help_mode_returns_false() {
            let mut state = create_test_state();
            state.modal.set_mode(InputMode::Help);

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
