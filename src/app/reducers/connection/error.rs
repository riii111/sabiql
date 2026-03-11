use std::time::Instant;

use crate::app::action::Action;
use crate::app::connection_state::ConnectionState;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;
use crate::domain::MetadataState;

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ShowConnectionError(info) => {
            state.connection_error.set_error(info.clone());
            state.ui.input_mode = InputMode::ConnectionError;
            Some(vec![])
        }
        Action::CloseConnectionError => {
            state.connection_error.details_expanded = false;
            state.connection_error.scroll_offset = 0;
            state.connection_error.clear_copied_feedback();
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ToggleConnectionErrorDetails => {
            state.connection_error.toggle_details();
            Some(vec![])
        }
        Action::ScrollConnectionErrorUp => {
            state.connection_error.scroll_up();
            Some(vec![])
        }
        Action::ScrollConnectionErrorDown => {
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
            if state.runtime.is_service_connection() {
                return Some(vec![]);
            }
            state.connection_error.clear();
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.cache.state = MetadataState::NotLoaded;
            state.ui.input_mode = InputMode::ConnectionSetup;
            Some(vec![])
        }
        Action::RetryServiceConnection => {
            if let Some(dsn) = state.runtime.dsn.clone() {
                state.connection_error.clear();
                state.runtime.connection_state = ConnectionState::Connecting;
                state.cache.state = MetadataState::Loading;
                state.ui.input_mode = InputMode::Normal;
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
            state.runtime.dsn = Some("service=mydb".to_string());
            state.ui.input_mode = InputMode::ConnectionError;

            reduce(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
        }

        #[test]
        fn allowed_for_profile_connection() {
            let mut state = AppState::new("test".to_string());
            state.runtime.dsn = Some("postgres://localhost/db".to_string());
            state.ui.input_mode = InputMode::ConnectionError;

            reduce(&mut state, &Action::ReenterConnectionSetup, Instant::now());

            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
        }
    }
}
