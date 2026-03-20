use std::time::Instant;

use crate::app::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::app::connection_state::ConnectionState;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;
use crate::domain::MetadataState;

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ShowConnectionError(info) => {
            state.connection_error.set_error(info.clone());
            state.modal.replace_mode(InputMode::ConnectionError);
            Some(vec![])
        }
        Action::CloseConnectionError => {
            state.connection_error.details_expanded = false;
            state.connection_error.scroll_offset = 0;
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
            state.connection_error.scroll_down(100);
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
            state
                .session
                .set_connection_state(ConnectionState::NotConnected);
            state.session.set_metadata_state(MetadataState::NotLoaded);
            state.modal.replace_mode(InputMode::ConnectionSetup);
            Some(vec![])
        }
        Action::RetryServiceConnection => {
            if let Some(dsn) = state.session.dsn.clone() {
                state.connection_error.clear();
                state.session.begin_connecting(&dsn);
                state.session.read_only = false;
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

    mod reenter_connection_setup {
        use super::*;

        #[test]
        fn blocked_for_service_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.dsn = Some("service=mydb".to_string());
            state.modal.set_mode(InputMode::ConnectionError);

            reduce(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionError);
        }

        #[test]
        fn allowed_for_profile_connection() {
            let mut state = AppState::new("test".to_string());
            state.session.dsn = Some("postgres://localhost/db".to_string());
            state.modal.set_mode(InputMode::ConnectionError);

            reduce(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        }
    }
}
