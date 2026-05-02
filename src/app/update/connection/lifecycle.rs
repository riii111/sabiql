use crate::cmd::effect::Effect;
use crate::domain::connection::{ConnectionId, DatabaseType};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ConnectionTarget};

use super::helpers::{restore_cache, save_current_cache};

fn reset_for_new_connection(
    state: &mut AppState,
    id: &ConnectionId,
    dsn: &str,
    name: &str,
    database_type: DatabaseType,
) {
    state.session.reset(&mut state.query);
    state.result_interaction.reset_view();
    state.ui.set_explorer_selection(None);
    state
        .session
        .set_active_connection(id, name, database_type, dsn);
    state.session.disable_read_only();
}

pub fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    match action {
        Action::TryConnect => {
            if state.session.connection_state().is_not_connected()
                && state.modal.active_mode() == InputMode::Normal
            {
                if let Some(dsn) = state.session.dsn().map(str::to_string) {
                    state.session.begin_connecting(&dsn);
                    Some(vec![Effect::FetchMetadata { dsn }])
                } else {
                    Some(vec![])
                }
            } else {
                Some(vec![])
            }
        }

        Action::SwitchConnection(ConnectionTarget {
            id,
            dsn,
            name,
            database_type,
        }) => {
            if let Some(current_id) = state.session.active_connection_id().cloned() {
                let cache = save_current_cache(state);
                state.connection_caches.save(&current_id, cache);
            }

            if let Some(cached) = state.connection_caches.get(id).cloned() {
                restore_cache(state, &cached, id, name, *database_type, dsn);
                Some(vec![Effect::ClearCompletionEngineCache])
            } else {
                // No cache: reset and fetch metadata
                reset_for_new_connection(state, id, dsn, name, *database_type);
                state.session.begin_connecting(dsn);
                Some(vec![
                    Effect::ClearCompletionEngineCache,
                    Effect::FetchMetadata { dsn: dsn.clone() },
                ])
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ConnectionId;
    use crate::model::connection::cache::ConnectionCache;
    use crate::model::connection::state::ConnectionState;
    use crate::model::shared::inspector_tab::InspectorTab;

    fn create_switch_action(id: &ConnectionId, name: &str) -> Action {
        Action::SwitchConnection(ConnectionTarget {
            id: id.clone(),
            dsn: format!("postgres://localhost/{name}"),
            name: name.to_string(),
            database_type: DatabaseType::PostgreSQL,
        })
    }

    #[test]
    fn saves_current_cache_before_switching() {
        let mut state = AppState::new("test".to_string());
        let current_id = ConnectionId::new();
        let new_id = ConnectionId::new();

        state
            .session
            .set_active_connection_id_for_test(Some(current_id.clone()));
        state.ui.set_explorer_selected_raw(5);
        state.ui.set_inspector_tab(InspectorTab::Indexes);

        let action = create_switch_action(&new_id, "new_db");
        reduce(&mut state, &action);

        let saved = state.connection_caches.get(&current_id).unwrap();
        assert_eq!(saved.explorer_selected, 5);
        assert_eq!(saved.inspector_tab, InspectorTab::Indexes);
    }

    #[test]
    fn restores_cached_state_when_available() {
        let mut state = AppState::new("test".to_string());
        let target_id = ConnectionId::new();

        let cached = ConnectionCache {
            explorer_selected: 42,
            inspector_tab: InspectorTab::ForeignKeys,
            ..Default::default()
        };
        state.connection_caches.save(&target_id, cached);

        let action = create_switch_action(&target_id, "cached_db");
        reduce(&mut state, &action);

        assert_eq!(state.ui.explorer_selected(), 42);
        assert_eq!(state.ui.inspector_tab(), InspectorTab::ForeignKeys);
        assert_eq!(
            state.session.active_database_type(),
            Some(DatabaseType::PostgreSQL)
        );
    }

    #[test]
    fn normalizes_cached_inspector_tab_when_capability_is_missing() {
        let mut state = AppState::new("test".to_string());
        let target_id = ConnectionId::new();
        let cached = ConnectionCache {
            explorer_selected: 42,
            inspector_tab: InspectorTab::Ddl,
            ..Default::default()
        };
        state.connection_caches.save(&target_id, cached);

        let action = Action::SwitchConnection(ConnectionTarget {
            id: target_id,
            dsn: "sqlite:///tmp/app.db".to_string(),
            name: "app.db".to_string(),
            database_type: DatabaseType::SQLite,
        });
        reduce(&mut state, &action);

        assert_eq!(state.ui.inspector_tab(), InspectorTab::Info);
    }

    #[test]
    fn fetches_metadata_when_no_cache_exists() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        let action = create_switch_action(&new_id, "fresh_db");
        let effects = reduce(&mut state, &action).unwrap();

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::FetchMetadata { .. }))
        );
        assert_eq!(
            state.session.connection_state(),
            ConnectionState::Connecting
        );
    }

    #[test]
    fn sqlite_switch_without_cache_fetches_metadata() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        let action = Action::SwitchConnection(ConnectionTarget {
            id: new_id,
            dsn: "sqlite:///tmp/app.db".to_string(),
            name: "app.db".to_string(),
            database_type: DatabaseType::SQLite,
        });
        let effects = reduce(&mut state, &action).unwrap();

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::FetchMetadata { .. }))
        );
        assert_eq!(
            state.session.connection_state(),
            ConnectionState::Connecting
        );
        assert_eq!(
            state.session.active_database_type(),
            Some(DatabaseType::SQLite)
        );
    }

    #[test]
    fn sqlite_switch_restores_cache() {
        let mut state = AppState::new("test".to_string());
        let target_id = ConnectionId::new();
        state.ui.set_explorer_selected_raw(7);
        state.connection_caches.save(
            &target_id,
            ConnectionCache {
                explorer_selected: 42,
                inspector_tab: InspectorTab::ForeignKeys,
                ..Default::default()
            },
        );

        let action = Action::SwitchConnection(ConnectionTarget {
            id: target_id.clone(),
            dsn: "sqlite:///tmp/app.db".to_string(),
            name: "app.db".to_string(),
            database_type: DatabaseType::SQLite,
        });
        let effects = reduce(&mut state, &action).unwrap();

        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::FetchMetadata { .. }))
        );
        assert!(state.connection_caches.get(&target_id).is_some());
        assert_eq!(state.ui.explorer_selected(), 42);
        assert_eq!(
            state.session.active_database_type(),
            Some(DatabaseType::SQLite)
        );
        assert_eq!(state.session.connection_state(), ConnectionState::Connected);
    }

    #[test]
    fn sqlite_try_connect_fetches_metadata() {
        let mut state = AppState::new("test".to_string());
        state.session.set_dsn_for_test("sqlite:///tmp/app.db");
        state
            .session
            .set_active_database_type_for_test(Some(DatabaseType::SQLite));
        state
            .session
            .set_connection_state(ConnectionState::NotConnected);

        let effects = reduce(&mut state, &Action::TryConnect).unwrap();

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::FetchMetadata { .. }))
        );
        assert_eq!(
            state.session.connection_state(),
            ConnectionState::Connecting
        );
    }

    #[test]
    fn updates_active_connection_fields() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        let action = create_switch_action(&new_id, "target_db");
        reduce(&mut state, &action);

        assert_eq!(state.session.active_connection_id(), Some(&new_id));
        assert_eq!(state.session.dsn(), Some("postgres://localhost/target_db"));
        assert_eq!(state.session.active_connection_name(), Some("target_db"));
        assert_eq!(
            state.session.active_database_type(),
            Some(DatabaseType::PostgreSQL)
        );
    }

    #[test]
    fn sets_connected_state_when_cache_exists() {
        let mut state = AppState::new("test".to_string());
        let target_id = ConnectionId::new();

        state
            .connection_caches
            .save(&target_id, ConnectionCache::default());

        let action = create_switch_action(&target_id, "cached_db");
        reduce(&mut state, &action);

        assert_eq!(state.session.connection_state(), ConnectionState::Connected);
    }

    #[test]
    fn resets_result_selection_when_restoring_cache() {
        let mut state = AppState::new("test".to_string());
        let target_id = ConnectionId::new();

        state
            .connection_caches
            .save(&target_id, ConnectionCache::default());
        state.result_interaction.activate_cell(3, 2);

        let action = create_switch_action(&target_id, "cached_db");
        reduce(&mut state, &action);

        assert_eq!(
            state.result_interaction.selection().mode(),
            crate::model::shared::ui_state::ResultNavMode::Scroll
        );
    }

    #[test]
    fn resets_result_selection_when_no_cache() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        state.result_interaction.activate_cell(5, 0);

        let action = create_switch_action(&new_id, "fresh_db");
        reduce(&mut state, &action);

        assert_eq!(
            state.result_interaction.selection().mode(),
            crate::model::shared::ui_state::ResultNavMode::Scroll
        );
    }

    #[test]
    fn resets_read_only_on_switch() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();
        state.session.enable_read_only();

        let action = create_switch_action(&new_id, "fresh_db");
        reduce(&mut state, &action);

        assert!(!state.session.is_read_only());
    }

    #[test]
    fn clears_completion_cache_on_switch() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        let action = create_switch_action(&new_id, "any_db");
        let effects = reduce(&mut state, &action).unwrap();

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
        );
    }
}
