use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ShowConnectionError(info) => {
            state.connection_error.set_error(info.clone());
            state.modal.replace_mode(InputMode::ConnectionError);
            Some(vec![])
        }
        Action::CloseConnectionError => {
            state.connection_error.reset_view();
            state.connection_error.clear_copied_feedback();
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }
        Action::ToggleConnectionErrorDetails => {
            state.connection_error.toggle_details();
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::ConnectionError,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.connection_error.scroll_up();
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::ConnectionError,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            // Approximation: uses raw line count, not wrapped line count.
            // Long lines that wrap in the UI may under-count; visible_height
            // is not subtracted. Acceptable for typical short psql errors.
            let max_scroll = state.connection_error.detail_line_count().saturating_sub(1);
            state.connection_error.scroll_down(max_scroll);
            Some(vec![])
        }
        Action::CopyConnectionError => {
            if let Some(content) = state.connection_error.masked_details() {
                Some(vec![Effect::CopyToClipboard {
                    content: content.to_string(),
                    on_success: Some(Action::ConnectionErrorCopied),
                    on_failure: None,
                }])
            } else {
                Some(vec![])
            }
        }
        Action::ConnectionErrorCopied => {
            state.connection_error.mark_copied_at(now);
            Some(vec![])
        }
        Action::ReenterConnectionSetup => {
            if state.session.is_service_connection() {
                return Some(vec![]);
            }
            state.connection_error.clear();
            state.session.mark_disconnected();
            state.modal.replace_mode(InputMode::ConnectionSetup);
            Some(vec![])
        }
        Action::RetryServiceConnection => {
            if let Some(dsn) = state.session.dsn().map(str::to_string) {
                state.connection_error.clear();
                state.session.begin_connecting(&dsn);
                state.session.disable_read_only();
                state.modal.set_mode(InputMode::Normal);
                Some(vec![Effect::FetchMetadata { dsn }])
            } else {
                Some(vec![])
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod scroll_down {
        use super::*;
        use crate::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};

        fn scroll_down_action() -> Action {
            Action::Scroll {
                target: ScrollTarget::ConnectionError,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        }

        #[test]
        fn stops_at_detail_line_count() {
            let mut state = AppState::new("test".to_string());
            state
                .connection_error
                .set_error(ConnectionErrorInfo::with_kind(
                    ConnectionErrorKind::Unknown,
                    "line1\nline2\nline3",
                ));

            let action = scroll_down_action();
            let now = Instant::now();

            reduce(&mut state, &action, now);
            reduce(&mut state, &action, now);
            assert_eq!(state.connection_error.scroll_offset(), 2);

            reduce(&mut state, &action, now);
            assert_eq!(state.connection_error.scroll_offset(), 2);
        }
    }

    mod reenter_connection_setup {
        use super::*;

        #[test]
        fn blocked_for_service_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.set_dsn_for_test("service=mydb");
            state.modal.set_mode(InputMode::ConnectionError);

            reduce(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionError);
        }

        #[test]
        fn allowed_for_profile_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.set_dsn_for_test("postgres://localhost/db");
            state.modal.set_mode(InputMode::ConnectionError);

            reduce(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        }
    }
}
