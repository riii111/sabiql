use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::services::AppServices;
use crate::update::action::Action;

pub fn reduce(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    _now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        Action::SetFocusedPane(pane) => {
            if *pane != FocusedPane::Result {
                state.result_interaction.reset_interaction();
                if state.modal.active_mode() == InputMode::CellEdit {
                    state.modal.set_mode(InputMode::Normal);
                }
            }
            state.ui.set_focused_pane(*pane);
            Some(vec![])
        }
        Action::ToggleFocus => {
            let was_focus = state.ui.is_focus_mode();
            state.toggle_focus();
            if was_focus {
                state.result_interaction.reset_interaction();
            }
            Some(vec![])
        }
        Action::ToggleReadOnly => {
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
            Some(vec![])
        }
        Action::InspectorNextTab => {
            state.ui.inspector_tab = services
                .db_capabilities
                .next_inspector_tab(state.ui.inspector_tab);
            Some(vec![])
        }
        Action::InspectorPrevTab => {
            state.ui.inspector_tab = services
                .db_capabilities
                .prev_inspector_tab(state.ui.inspector_tab);
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::db_capabilities::DbCapabilities;
    use crate::model::shared::inspector_tab::InspectorTab;
    use crate::services::AppServices;
    use crate::update::browse::navigation::reduce_navigation;

    mod toggle_read_only {
        use super::*;

        #[test]
        fn rw_to_ro_switches_immediately() {
            let mut state = AppState::new("test".to_string());
            assert!(!state.session.is_read_only());

            reduce_navigation(
                &mut state,
                &Action::ToggleReadOnly,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.session.is_read_only());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn ro_to_rw_opens_confirm_dialog() {
            let mut state = AppState::new("test".to_string());
            state.session.enable_read_only();

            reduce_navigation(
                &mut state,
                &Action::ToggleReadOnly,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.session.is_read_only());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(ConfirmIntent::DisableReadOnly)
            ));
        }
    }

    mod inspector_tabs {
        use super::*;

        fn services_with_two_tabs() -> AppServices {
            let mut services = AppServices::stub();
            services.db_capabilities =
                DbCapabilities::new(true, vec![InspectorTab::Info, InspectorTab::Columns]);
            services
        }

        #[test]
        fn next_tab_wraps_between_supported_tabs() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_tab = InspectorTab::Info;

            reduce_navigation(
                &mut state,
                &Action::InspectorNextTab,
                &services_with_two_tabs(),
                Instant::now(),
            );

            assert_eq!(state.ui.inspector_tab, InspectorTab::Columns);
        }

        #[test]
        fn prev_tab_wraps_between_supported_tabs() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_tab = InspectorTab::Info;

            reduce_navigation(
                &mut state,
                &Action::InspectorPrevTab,
                &services_with_two_tabs(),
                Instant::now(),
            );

            assert_eq!(state.ui.inspector_tab, InspectorTab::Columns);
        }
    }
}
