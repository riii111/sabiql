use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_connection_error(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::ShowConnectionError(info) => {
            state.connection_error.set_error(info.clone());
            state.modal.replace_mode(InputMode::ConnectionError);
            DispatchResult::handled()
        }
        Action::CloseConnectionError => {
            state.connection_error.details_expanded = false;
            state.connection_error.scroll_offset = 0;
            state.connection_error.clear_copied_feedback();
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }
        Action::ToggleConnectionErrorDetails => {
            state.connection_error.toggle_details();
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::ConnectionError,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.connection_error.scroll_up();
            DispatchResult::handled()
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
            DispatchResult::handled()
        }
        Action::CopyConnectionError => {
            if let Some(content) = state.connection_error.masked_details() {
                DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                    content: content.to_string(),
                    on_success: Some(Action::ConnectionErrorCopied),
                    on_failure: None,
                }])
            } else {
                DispatchResult::handled()
            }
        }
        Action::ConnectionErrorCopied => {
            state.connection_error.mark_copied_at(now);
            DispatchResult::handled()
        }
        Action::ReenterConnectionSetup => {
            if state.session.is_service_connection() {
                return DispatchResult::handled();
            }
            state.connection_error.clear();
            state.session.mark_disconnected();
            state.modal.replace_mode(InputMode::ConnectionSetup);
            DispatchResult::handled()
        }
        Action::RetryServiceConnection => {
            if let Some(dsn) = state.session.dsn.clone() {
                state.connection_error.clear();
                state.session.begin_connecting(&dsn);
                state.session.read_only = false;
                state.modal.set_mode(InputMode::Normal);
                DispatchResult::handled_with(vec![Effect::FetchMetadata { dsn }])
            } else {
                DispatchResult::handled()
            }
        }

        _ => DispatchResult::pass(),
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

            reduce_connection_error(&mut state, &action, now);
            reduce_connection_error(&mut state, &action, now);
            assert_eq!(state.connection_error.scroll_offset, 2);

            reduce_connection_error(&mut state, &action, now);
            assert_eq!(state.connection_error.scroll_offset, 2);
        }
    }

    mod reenter_connection_setup {
        use super::*;

        #[test]
        fn blocked_for_service_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.dsn = Some("service=mydb".to_string());
            state.modal.set_mode(InputMode::ConnectionError);

            reduce_connection_error(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionError);
        }

        #[test]
        fn allowed_for_profile_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.dsn = Some("postgres://localhost/db".to_string());
            state.modal.set_mode(InputMode::ConnectionError);

            reduce_connection_error(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        }
    }
}
