use std::time::Instant;

use crate::app::action::{Action, ConnectionsLoadedPayload};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::ConnectionListSelectNext => {
            let len = state.connection_list_items().len();
            let next = state.ui.connection_list_selected + 1;
            if next < len {
                state.ui.set_connection_list_selection(Some(next));
            }
            Some(vec![])
        }
        Action::ConnectionListSelectPrevious => {
            if state.ui.connection_list_selected > 0 {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected - 1));
            }
            Some(vec![])
        }
        Action::ConnectionsLoaded(ConnectionsLoadedPayload {
            profiles,
            services,
            service_file_path,
            profile_load_warning,
            service_load_warning,
        }) => {
            let mut sorted = profiles.clone();
            sorted.sort_by(|a, b| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            });
            state.set_connections_and_services(sorted, services.clone());
            state.runtime.service_file_path = service_file_path.clone();

            if let Some(warning) = profile_load_warning {
                state.messages.set_error_at(warning.clone(), now);
            }
            if let Some(warning) = service_load_warning {
                state.messages.set_error_at(warning.clone(), now);
            }

            let list_len = state.connection_list_items().len();
            if list_len == 0 {
                state.ui.set_connection_list_selection(Some(0));
            } else if state.ui.connection_list_selected >= list_len {
                state
                    .ui
                    .set_connection_list_selection(Some(list_len.saturating_sub(1)));
            } else {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected));
            }
            Some(vec![])
        }
        Action::ConfirmConnectionSelection => {
            use crate::app::connection_list::ConnectionListItem;
            let selected_idx = state.ui.connection_list_selected;

            let effect = match state.connection_list_items().get(selected_idx) {
                Some(ConnectionListItem::Profile(i)) => state
                    .connections()
                    .get(*i)
                    .filter(|c| state.session.active_connection_id.as_ref() != Some(&c.id))
                    .map(|_| Effect::SwitchConnection {
                        connection_index: *i,
                    }),
                Some(ConnectionListItem::Service(i)) => {
                    Some(Effect::SwitchToService { service_index: *i })
                }
                _ => None,
            };

            state.modal.set_mode(InputMode::Normal);

            match effect {
                Some(e) => Some(vec![e]),
                None => Some(vec![]),
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::reducers::navigation::reduce_navigation;
    use crate::app::services::AppServices;
    use crate::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

    fn create_test_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: ConnectionId::new(),
            name: ConnectionName::new(name).unwrap(),
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            ssl_mode: SslMode::Prefer,
        }
    }

    mod connection_list_navigation {
        use super::*;

        fn setup_profiles(state: &mut AppState, count: usize) {
            let names: Vec<String> = (1..=count).map(|i| format!("conn{}", i)).collect();
            let profiles = names.iter().map(|n| create_test_profile(n)).collect();
            state.set_connections(profiles);
        }

        #[test]
        fn select_next_increments_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 3);
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_next_stops_at_last() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_previous_decrements_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn select_previous_stops_at_first() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 1);
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }
    }

    mod connections_loaded {
        use super::*;

        #[test]
        fn sorts_connections_by_name_case_insensitive() {
            let mut state = AppState::new("test".to_string());
            let profiles = vec![
                create_test_profile("Zebra"),
                create_test_profile("alpha"),
                create_test_profile("Beta"),
            ];

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.connections()[0].display_name(), "alpha");
            assert_eq!(state.connections()[1].display_name(), "Beta");
            assert_eq!(state.connections()[2].display_name(), "Zebra");
        }

        #[test]
        fn initializes_selection_when_not_empty() {
            let mut state = AppState::new("test".to_string());
            let profiles = vec![create_test_profile("conn1")];

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn stores_service_file_path_in_runtime() {
            let mut state = AppState::new("test".to_string());
            let path = std::path::PathBuf::from("/etc/pg_service.conf");

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: Some(path.clone()),
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.runtime.service_file_path, Some(path));
        }

        #[test]
        fn service_load_warning_sets_error_message() {
            let mut state = AppState::new("test".to_string());

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: Some("parse error at line 5".to_string()),
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.messages.last_error.is_some());
        }
    }

    mod confirm_connection_selection {
        use super::*;

        fn create_test_profile_with_id(name: &str, id: ConnectionId) -> ConnectionProfile {
            ConnectionProfile {
                id,
                name: ConnectionName::new(name).unwrap(),
                host: "localhost".to_string(),
                port: 5432,
                database: "test".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: SslMode::Prefer,
            }
        }

        #[test]
        fn different_connection_dispatches_switch_effect() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.set_connections(vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ]);
            state.session.active_connection_id = Some(active_id);
            state.ui.set_connection_list_selection(Some(1));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                &effects[0],
                Effect::SwitchConnection {
                    connection_index: 1
                }
            ));
        }

        #[test]
        fn stays_on_same_connection_returns_to_tables() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.set_connections(vec![create_test_profile_with_id(
                "active",
                active_id.clone(),
            )]);
            state.session.active_connection_id = Some(active_id);
            state.ui.set_connection_list_selection(Some(0));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn empty_connections_returns_empty_effects() {
            let mut state = AppState::new("test".to_string());

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }

        #[test]
        fn from_selector_mode_switches_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.set_connections(vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ]);
            state.session.active_connection_id = Some(active_id);
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(1));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects
                .iter()
                .any(|e| matches!(e, Effect::SwitchConnection { connection_index } if *connection_index == 1)));
        }

        #[test]
        fn from_selector_same_connection_returns_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.set_connections(vec![create_test_profile_with_id(
                "active",
                active_id.clone(),
            )]);
            state.session.active_connection_id = Some(active_id);
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(0));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }
    }
}
