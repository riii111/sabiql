use crate::model::app_state::AppState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn reduce_focus(state: &mut AppState, action: &Action) -> DispatchResult {
    match action {
        Action::SetFocusedPane(pane) => {
            if *pane != FocusedPane::Result {
                state.result_interaction.reset_interaction();
                if state.modal.active_mode() == InputMode::CellEdit {
                    state.modal.set_mode(InputMode::Normal);
                }
            }
            state.ui.set_focused_pane(*pane);
            DispatchResult::handled()
        }
        Action::ToggleFocus => {
            let was_focus = state.ui.is_focus_mode();
            state.toggle_focus();
            if was_focus {
                state.result_interaction.reset_interaction();
            }
            DispatchResult::handled()
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
            DispatchResult::handled()
        }
        Action::InspectorNextTab => {
            state.ui.set_inspector_tab(
                state
                    .session
                    .active_db_capabilities()
                    .next_inspector_tab(state.ui.inspector_tab()),
            );
            DispatchResult::handled()
        }
        Action::InspectorPrevTab => {
            state.ui.set_inspector_tab(
                state
                    .session
                    .active_db_capabilities()
                    .prev_inspector_tab(state.ui.inspector_tab()),
            );
            DispatchResult::handled()
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConnectionId, DatabaseType};
    use crate::model::shared::inspector_tab::InspectorTab;
    use crate::services::AppServices;
    use crate::update::browse::navigation::dispatch_navigation;
    use std::time::Instant;

    mod toggle_read_only {
        use super::*;

        #[test]
        fn rw_to_ro_switches_immediately() {
            let mut state = AppState::new("test".to_string());
            assert!(!state.session.is_read_only());

            dispatch_navigation(
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

            dispatch_navigation(
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

        fn use_sqlite_connection(state: &mut AppState) {
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite://test.db",
            );
        }

        #[test]
        fn next_tab_uses_session_capabilities() {
            let mut state = AppState::new("test".to_string());
            use_sqlite_connection(&mut state);
            state.ui.set_inspector_tab(InspectorTab::Info);

            dispatch_navigation(
                &mut state,
                &Action::InspectorNextTab,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.inspector_tab(), InspectorTab::Columns);
        }

        #[test]
        fn prev_tab_uses_session_capabilities() {
            let mut state = AppState::new("test".to_string());
            use_sqlite_connection(&mut state);
            state.ui.set_inspector_tab(InspectorTab::Info);

            dispatch_navigation(
                &mut state,
                &Action::InspectorPrevTab,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.inspector_tab(), InspectorTab::Ddl);
        }
    }
}
