use std::time::Instant;

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::update::action::Action;

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenConnectionSelector => {
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(0));
            Some(vec![Effect::LoadConnections])
        }

        // ===== Connection Deletion =====
        Action::RequestDeleteSelectedConnection => {
            use crate::app::model::connection::list::ConnectionListItem;
            let selected_idx = state.ui.connection_list_selected;
            let profile_idx = match state.connection_list_items().get(selected_idx) {
                Some(ConnectionListItem::Profile(i)) => *i,
                _ => return Some(vec![]),
            };
            if let Some(connection) = state.connections().get(profile_idx) {
                let id = connection.id.clone();
                let name = connection.name.as_str().to_string();
                let is_active = state.session.active_connection_id.as_ref() == Some(&id);

                let message = if is_active {
                    format!(
                        "Delete \"{name}\"?\n\n\u{26A0} This is the active connection.\nYou will be disconnected.\n\nThis action cannot be undone."
                    )
                } else {
                    format!("Delete \"{name}\"?\n\nThis action cannot be undone.")
                };
                state.confirm_dialog.open(
                    "Delete Connection",
                    message,
                    crate::app::model::shared::confirm_dialog::ConfirmIntent::DeleteConnection(id),
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
            }
            Some(vec![])
        }
        Action::DeleteConnection(id) => Some(vec![Effect::DeleteConnection { id: id.clone() }]),
        Action::ConnectionDeleted(id) => {
            if state.session.active_connection_id.as_ref() == Some(id) {
                state.session.reset(&mut state.query);
                state.result_interaction.reset_view();
                state.ui.set_explorer_selection(None);
            }

            let id_clone = id.clone();
            state.retain_connections(move |c| c.id != id_clone);
            state.connection_caches.remove(id);

            let list_len = state.connection_list_items().len();
            if state.ui.connection_list_selected >= list_len && list_len > 0 {
                state.ui.set_connection_list_selection(Some(list_len - 1));
            }

            if state.connections().is_empty() && state.service_entries().is_empty() {
                state.connection_setup.reset();
                state.connection_setup.is_first_run = false;
                state.modal.set_mode(InputMode::ConnectionSetup);
            }

            state
                .messages
                .set_success_at("Connection deleted".to_string(), now);
            Some(vec![])
        }
        Action::ConnectionDeleteFailed(e) => {
            state.messages.set_error_at(e.to_string(), now);
            Some(vec![])
        }

        // ===== Connection Edit =====
        Action::RequestEditSelectedConnection => {
            use crate::app::model::connection::list::ConnectionListItem;
            let selected_idx = state.ui.connection_list_selected;
            let profile_idx = match state.connection_list_items().get(selected_idx) {
                Some(ConnectionListItem::Profile(i)) => *i,
                _ => return Some(vec![]),
            };
            if let Some(connection) = state.connections().get(profile_idx) {
                let id = connection.id.clone();
                Some(vec![Effect::LoadConnectionForEdit { id }])
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
    use crate::app::model::connection::list::build_connection_list;
    use crate::domain::connection::{ConnectionProfile, SslMode};

    fn create_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile::new(
            name.to_string(),
            "localhost".to_string(),
            5432,
            "db".to_string(),
            "user".to_string(),
            "pass".to_string(),
            SslMode::default(),
        )
        .unwrap()
    }

    mod open_connection_selector {
        use super::*;

        #[test]
        fn sets_mode_and_loads_connections() {
            let mut state = AppState::new("test".to_string());

            let effects = reduce(&mut state, &Action::OpenConnectionSelector, Instant::now());

            assert_eq!(state.input_mode(), InputMode::ConnectionSelector);
            let effects = effects.unwrap();
            assert!(effects.iter().any(|e| matches!(e, Effect::LoadConnections)));
        }

        #[test]
        fn resets_selection_to_zero() {
            let mut state = AppState::new("test".to_string());
            state.ui.set_connection_list_selection(Some(3));

            reduce(&mut state, &Action::OpenConnectionSelector, Instant::now());

            assert_eq!(state.ui.connection_list_selected, 0);
        }
    }

    mod request_delete_selected_connection {
        use super::*;

        #[test]
        fn opens_confirm_dialog_with_correct_message() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            state.set_connections(vec![profile]);
            state.ui.connection_list_selected = 0;

            reduce(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert_eq!(state.confirm_dialog.title(), "Delete Connection");
            assert!(state.confirm_dialog.message().contains("Production"));
            assert!(
                state
                    .confirm_dialog
                    .message()
                    .contains("This action cannot be undone")
            );
        }

        #[test]
        fn active_connection_shows_warning() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            let profile_id = profile.id.clone();
            state.set_connections(vec![profile]);
            state.ui.connection_list_selected = 0;
            state.session.active_connection_id = Some(profile_id);

            reduce(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert!(
                state
                    .confirm_dialog
                    .message()
                    .contains("This is the active connection")
            );
            assert!(
                state
                    .confirm_dialog
                    .message()
                    .contains("You will be disconnected")
            );
        }

        #[test]
        fn inactive_connection_shows_standard_message() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            state.set_connections(vec![profile]);
            state.ui.connection_list_selected = 0;

            reduce(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert!(
                !state
                    .confirm_dialog
                    .message()
                    .contains("This is the active connection")
            );
        }

        #[test]
        fn empty_list_does_nothing() {
            let mut state = AppState::new("test".to_string());
            state.set_connections(vec![]);
            state.modal.set_mode(InputMode::Normal);

            reduce(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn preserves_return_mode_from_connection_selector() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            state.set_connections(vec![profile]);
            state.ui.connection_list_selected = 0;
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.modal.set_mode(InputMode::ConnectionSelector);

            reduce(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert_eq!(
                state.modal.return_destination(),
                InputMode::ConnectionSelector
            );
        }
    }

    mod connection_deleted {
        use super::*;
        use crate::app::model::connection::state::ConnectionState;

        #[test]
        fn removes_connection_from_list() {
            let mut state = AppState::new("test".to_string());
            let profile1 = create_profile("First");
            let profile2 = create_profile("Second");
            let id_to_delete = profile1.id.clone();
            state.set_connections(vec![profile1, profile2]);

            reduce(
                &mut state,
                &Action::ConnectionDeleted(id_to_delete),
                Instant::now(),
            );

            assert_eq!(state.connections().len(), 1);
            assert_eq!(state.connections()[0].name.as_str(), "Second");
        }

        #[test]
        fn clears_active_state_when_active_deleted() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            let profile_id = profile.id.clone();
            state.set_connections(vec![profile]);
            state.session.active_connection_id = Some(profile_id.clone());
            state.session.dsn = Some("postgres://localhost/db".to_string());
            state
                .session
                .set_connection_state(ConnectionState::Connected);

            reduce(
                &mut state,
                &Action::ConnectionDeleted(profile_id),
                Instant::now(),
            );

            assert!(state.session.active_connection_id.is_none());
            assert!(state.session.dsn.is_none());
            assert!(state.session.connection_state().is_not_connected());
        }

        #[test]
        fn resets_full_state_when_active_deleted() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            let profile_id = profile.id.clone();
            state.set_connections(vec![profile]);
            state.session.active_connection_id = Some(profile_id.clone());
            state.session.dsn = Some("postgres://localhost/db".to_string());
            state
                .session
                .set_connection_state(ConnectionState::Connected);

            // Set state that was previously not reset by ConnectionDeleted
            state.query.enter_history(2);
            state.query.pagination.current_page = 3;
            state.result_interaction.activate_cell(5, 0);
            state.result_interaction.scroll_offset = 10;
            state.result_interaction.horizontal_offset = 20;
            state.result_interaction.stage_row(0);

            reduce(
                &mut state,
                &Action::ConnectionDeleted(profile_id),
                Instant::now(),
            );

            assert!(state.query.history_index().is_none());
            assert_eq!(state.query.pagination.current_page, 0);
            assert_eq!(
                state.result_interaction.selection().mode(),
                crate::app::model::shared::ui_state::ResultNavMode::Scroll
            );
            assert_eq!(state.result_interaction.scroll_offset, 0);
            assert_eq!(state.result_interaction.horizontal_offset, 0);
            assert!(state.result_interaction.staged_delete_rows().is_empty());
            assert!(state.result_interaction.pending_write_preview().is_none());
        }

        #[test]
        fn adjusts_selection_when_last_item_deleted() {
            let mut state = AppState::new("test".to_string());
            let profile1 = create_profile("First");
            let profile2 = create_profile("Second");
            let id_to_delete = profile2.id.clone();
            state.set_connections(vec![profile1, profile2]);
            state.ui.connection_list_selected = 1;

            reduce(
                &mut state,
                &Action::ConnectionDeleted(id_to_delete),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn transitions_to_setup_when_list_empty() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Only");
            let profile_id = profile.id.clone();
            state.set_connections(vec![profile]);

            reduce(
                &mut state,
                &Action::ConnectionDeleted(profile_id),
                Instant::now(),
            );

            assert!(state.connections().is_empty());
            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        }

        #[test]
        fn rebuilds_connection_list_items_after_delete() {
            let mut state = AppState::new("test".to_string());
            let profile1 = create_profile("First");
            let profile2 = create_profile("Second");
            let id_to_delete = profile1.id.clone();
            state.set_connections(vec![profile1, profile2]);

            reduce(
                &mut state,
                &Action::ConnectionDeleted(id_to_delete),
                Instant::now(),
            );

            assert_eq!(state.connection_list_items(), build_connection_list(1, 0));
        }

        #[test]
        fn stays_in_selector_when_services_remain_after_last_profile_deleted() {
            use crate::domain::connection::ServiceEntry;

            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Only");
            let profile_id = profile.id.clone();
            state.set_connections_and_services(
                vec![profile],
                vec![ServiceEntry {
                    service_name: "mydb".to_string(),
                    host: None,
                    dbname: None,
                    port: None,
                    user: None,
                }],
            );
            state.modal.set_mode(InputMode::Normal);

            reduce(
                &mut state,
                &Action::ConnectionDeleted(profile_id),
                Instant::now(),
            );

            assert!(state.connections().is_empty());
            assert_ne!(state.input_mode(), InputMode::ConnectionSetup);
            assert_eq!(state.connection_list_items(), build_connection_list(0, 1));
        }
    }
}
