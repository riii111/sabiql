mod error;
mod helpers;
mod lifecycle;
mod selector;
mod setup;

use std::time::Instant;

use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn reduce_connection(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    lifecycle::reduce(state, action, now, services)
        .or_else(|| setup::reduce(state, action, now))
        .or_else(|| error::reduce(state, action, now))
        .or_else(|| selector::reduce(state, action, now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::input_mode::InputMode;

    #[test]
    fn paste_handled_by_setup_in_connection_setup_mode() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::ConnectionSetup);

        let result = reduce_connection(
            &mut state,
            &Action::Paste("hello".to_string()),
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(result.is_handled());
    }

    #[test]
    fn paste_falls_through_in_normal_mode() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);

        let result = reduce_connection(
            &mut state,
            &Action::Paste("hello".to_string()),
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(result.is_pass());
    }

    #[test]
    fn unknown_action_returns_none() {
        let mut state = AppState::new("test".to_string());

        let result = reduce_connection(
            &mut state,
            &Action::Quit,
            Instant::now(),
            &AppServices::stub(),
        );

        assert!(result.is_pass());
    }
}
