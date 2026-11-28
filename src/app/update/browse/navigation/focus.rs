use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;

pub(in crate::update) fn toggle_read_only(state: &mut AppState) -> Vec<Effect> {
    if state.session.is_read_only() {
        state.confirm_dialog.open(
            "Disable Read-Only",
            "Switch to read-write mode? Write operations will be allowed.",
            ConfirmIntent::DisableReadOnly,
        );
        state.modal.push_mode(InputMode::ConfirmDialog);
    } else {
        state.session.enable_read_only();
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::action::Action;
    use crate::update::reducer::route;
    use std::time::Instant;

    mod toggle_read_only {
        use super::*;

        #[test]
        fn rw_to_ro_switches_immediately() {
            let mut state = AppState::new("test".to_string());
            assert!(!state.session.is_read_only());

            route(&mut state, Action::ToggleReadOnly, Instant::now());

            assert!(state.session.is_read_only());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn ro_to_rw_opens_confirm_dialog() {
            let mut state = AppState::new("test".to_string());
            state.session.enable_read_only();

            route(&mut state, Action::ToggleReadOnly, Instant::now());

            assert!(state.session.is_read_only());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(ConfirmIntent::DisableReadOnly)
            ));
        }
    }
}
