use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;

pub(in crate::update) fn set_focused_pane(state: &mut AppState, pane: FocusedPane) -> Vec<Effect> {
    if pane != FocusedPane::Result {
        state.result_interaction.reset_interaction();
        if state.modal.active_mode() == InputMode::CellEdit {
            state.modal.set_mode(InputMode::Normal);
        }
    }
    state.ui.focused_pane = pane;
    vec![]
}

pub(in crate::update) fn toggle_focus(state: &mut AppState) -> Vec<Effect> {
    let was_focus = state.ui.is_focus_mode();
    state.ui.toggle_focus();
    if was_focus {
        state.result_interaction.reset_interaction();
    }
    vec![]
}

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
    use crate::domain::ConnectionId;
    use crate::model::shared::inspector_tab::InspectorTab;
    use crate::services::AppServices;
    use crate::test_support::endpoint;
    use crate::update::action::Action;
    use crate::update::reducer::route;
    use std::time::Instant;

    mod toggle_read_only {
        use super::*;

        #[test]
        fn rw_to_ro_switches_immediately() {
            let mut state = AppState::new("test".to_string());
            assert!(!state.session.is_read_only());

            route(
                &mut state,
                Action::ToggleReadOnly,
                Instant::now(),
                &AppServices::stub(),
            );

            assert!(state.session.is_read_only());
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn ro_to_rw_opens_confirm_dialog() {
            let mut state = AppState::new("test".to_string());
            state.session.enable_read_only();

            route(
                &mut state,
                Action::ToggleReadOnly,
                Instant::now(),
                &AppServices::stub(),
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

        fn activate_sqlite_connection(state: &mut AppState) {
            state.session.activate_connection(
                &ConnectionId::new(),
                "sqlite",
                &endpoint::sqlite("sqlite://test.db"),
            );
        }

        #[test]
        fn next_tab_uses_session_capabilities() {
            let mut state = AppState::new("test".to_string());
            activate_sqlite_connection(&mut state);
            state.ui.inspector_tab = InspectorTab::Info;

            route(
                &mut state,
                Action::InspectorNextTab,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_tab, InspectorTab::Columns);
        }

        #[test]
        fn prev_tab_uses_session_capabilities() {
            let mut state = AppState::new("test".to_string());
            activate_sqlite_connection(&mut state);
            state.ui.inspector_tab = InspectorTab::Info;

            route(
                &mut state,
                Action::InspectorPrevTab,
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_tab, InspectorTab::Ddl);
        }
    }
}
