use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{ConnectionsLoadedPayload, ListMotion};

pub(in crate::update) fn list_select_connection_list(
    state: &mut AppState,
    motion: ListMotion,
) -> Vec<Effect> {
    match motion {
        ListMotion::Next => {
            let len = state.connection_list_items().len();
            let next = state.ui.connection_list_selected() + 1;
            if next < len {
                state.ui.set_connection_list_selection(Some(next));
            }
        }
        ListMotion::Previous => {
            if state.ui.connection_list_selected() > 0 {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected() - 1));
            }
        }
    }
    vec![]
}

pub(in crate::update) fn connections_loaded(
    state: &mut AppState,
    payload: ConnectionsLoadedPayload,
) -> Vec<Effect> {
    let ConnectionsLoadedPayload {
        profiles,
        services,
        service_file_path,
        profile_load_warning,
        service_load_warning,
    } = payload;
    let mut sorted = profiles;
    sorted.sort_by(|a, b| {
        a.display_name()
            .to_lowercase()
            .cmp(&b.display_name().to_lowercase())
    });
    state.set_connections_and_services(sorted, services);
    state.set_service_file_path(service_file_path);

    if let Some(warning) = profile_load_warning {
        state.messages.set_error(warning);
    }
    if let Some(warning) = service_load_warning {
        state.messages.set_error(warning);
    }

    let list_len = state.connection_list_items().len();
    if list_len == 0 {
        state.ui.set_connection_list_selection(Some(0));
    } else if state.ui.connection_list_selected() >= list_len {
        state
            .ui
            .set_connection_list_selection(Some(list_len.saturating_sub(1)));
    } else {
        state
            .ui
            .set_connection_list_selection(Some(state.ui.connection_list_selected()));
    }
    vec![]
}

pub(in crate::update) fn confirm_connection_selection(state: &mut AppState) -> Vec<Effect> {
    use crate::model::connection::list::ConnectionListItem;
    let selected_idx = state.ui.connection_list_selected();

    let effect = match state.connection_list_items().get(selected_idx) {
        Some(ConnectionListItem::Profile(i)) => state
            .connections()
            .get(*i)
            .filter(|c| state.session.active_connection_id() != Some(&c.id))
            .map(|_| Effect::SwitchConnection {
                connection_index: *i,
            }),
        Some(ConnectionListItem::Service(i)) => Some(Effect::SwitchToService { service_index: *i }),
        _ => None,
    };

    state.modal.set_mode(InputMode::Normal);

    match effect {
        Some(e) => vec![e],
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::{ConnectionId, ConnectionProfile, SslMode};
    use crate::test_support::endpoint;
    use crate::update::action::{Action, ListTarget};
    use crate::update::reducer::route;
    use std::time::Instant;

    fn create_test_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile::new_postgres(
            name,
            "localhost",
            5432,
            "test",
            "user",
            "pass",
            SslMode::Prefer,
        )
        .unwrap()
    }

    mod connection_list_navigation {
        use super::*;

        fn setup_profiles(state: &mut AppState, count: usize) {
            let names: Vec<String> = (1..=count).map(|i| format!("conn{i}")).collect();
            let profiles = names.iter().map(|n| create_test_profile(n)).collect();
            state.set_connections(profiles);
        }

        #[test]
        fn select_next_increments_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 3);
            state.ui.set_connection_list_selection(Some(0));

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::ConnectionList,
                    motion: ListMotion::Next,
                },
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected(), 1);
        }

        #[test]
        fn select_next_stops_at_last() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::ConnectionList,
                    motion: ListMotion::Next,
                },
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected(), 1);
        }

        #[test]
        fn select_previous_decrements_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::ConnectionList,
                    motion: ListMotion::Previous,
                },
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected(), 0);
        }

        #[test]
        fn select_previous_stops_at_first() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 1);
            state.ui.set_connection_list_selection(Some(0));

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::ConnectionList,
                    motion: ListMotion::Previous,
                },
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected(), 0);
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

            route(
                &mut state,
                Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
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

            route(
                &mut state,
                Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected(), 0);
        }

        #[test]
        fn stores_service_file_path_in_app_state() {
            let mut state = AppState::new("test".to_string());
            let path = std::path::PathBuf::from("/etc/pg_service.conf");

            route(
                &mut state,
                Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: Some(path.clone()),
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                Instant::now(),
            );

            assert_eq!(state.service_file_path(), Some(path.as_path()));
        }

        #[test]
        fn service_load_warning_sets_error_message() {
            let mut state = AppState::new("test".to_string());

            route(
                &mut state,
                Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: Some("parse error at line 5".to_string()),
                }),
                Instant::now(),
            );

            assert!(state.messages.last_error().is_some());
        }
    }

    mod confirm_connection_selection {
        use super::*;

        fn create_test_profile_with_id(name: &str, id: ConnectionId) -> ConnectionProfile {
            ConnectionProfile::with_id_postgres(
                id,
                name,
                "localhost",
                5432,
                "test",
                "user",
                "pass",
                SslMode::Prefer,
            )
            .unwrap()
        }

        #[test]
        fn empty_connections_returns_empty_effects() {
            let mut state = AppState::new("test".to_string());

            let effects = route(
                &mut state,
                Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert!(effects.is_empty());
        }

        #[test]
        fn from_selector_mode_switches_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.set_connections(vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id),
            ]);
            state.session.activate_connection(
                &active_id,
                "active",
                &endpoint::postgres("postgres://localhost/test"),
            );
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(1));

            let effects = route(
                &mut state,
                Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(matches!(
                effects.as_slice(),
                [Effect::SwitchConnection {
                    connection_index: 1
                }]
            ));
        }

        #[test]
        fn from_selector_same_connection_returns_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.set_connections(vec![create_test_profile_with_id(
                "active",
                active_id.clone(),
            )]);
            state.session.activate_connection(
                &active_id,
                "active",
                &endpoint::postgres("postgres://localhost/test"),
            );
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(0));

            let effects = route(
                &mut state,
                Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);
            assert!(effects.is_empty());
        }
    }
}
