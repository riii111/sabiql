use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::confirm_dialog::ConfirmIntent;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::update::action::Action;

pub fn reduce(state: &mut AppState, action: &Action, _now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::SetFocusedPane(pane) => {
            if *pane != FocusedPane::Result {
                state.result_interaction.reset_interaction();
                if state.modal.active_mode() == InputMode::CellEdit {
                    state.modal.set_mode(InputMode::Normal);
                }
            }
            state.ui.focused_pane = *pane;
            Some(vec![])
        }
        Action::ToggleFocus => {
            let was_focus = state.ui.focus_mode;
            state.toggle_focus();
            if was_focus {
                state.result_interaction.reset_interaction();
            }
            Some(vec![])
        }
        Action::ToggleReadOnly => {
            if state.session.read_only {
                state.confirm_dialog.open(
                    "Disable Read-Only",
                    "Switch to read-write mode? Write operations will be allowed.",
                    ConfirmIntent::DisableReadOnly,
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
            } else {
                state.session.read_only = true;
            }
            Some(vec![])
        }
        Action::InspectorNextTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.next();
            Some(vec![])
        }
        Action::InspectorPrevTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.prev();
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::AppServices;
    use crate::app::update::browse::navigation::reduce_navigation;

    mod toggle_read_only {
        use super::*;

        #[test]
        fn rw_to_ro_switches_immediately() {
            let mut state = AppState::new("test".to_string());
            assert!(!state.session.read_only);

            reduce_navigation(
                &mut state,
                &Action::ToggleReadOnly,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.session.read_only);
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn ro_to_rw_opens_confirm_dialog() {
            let mut state = AppState::new("test".to_string());
            state.session.read_only = true;

            reduce_navigation(
                &mut state,
                &Action::ToggleReadOnly,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.session.read_only);
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(ConfirmIntent::DisableReadOnly)
            ));
        }
    }
}
