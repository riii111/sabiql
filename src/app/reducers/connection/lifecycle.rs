use std::time::Instant;

use crate::app::action::{Action, ConnectionTarget};
use crate::app::connection_state::ConnectionState;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;
use crate::domain::MetadataState;

use super::helpers::{reset_connection_state, restore_cache, save_current_cache};

pub fn reduce(state: &mut AppState, action: &Action, _now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::TryConnect => {
            if state.runtime.connection_state.is_not_connected()
                && state.ui.input_mode == InputMode::Normal
            {
                if let Some(dsn) = state.runtime.dsn.clone() {
                    state.runtime.connection_state = ConnectionState::Connecting;
                    state.cache.state = MetadataState::Loading;
                    Some(vec![Effect::FetchMetadata { dsn }])
                } else {
                    Some(vec![])
                }
            } else {
                Some(vec![])
            }
        }

        Action::SwitchConnection(ConnectionTarget { id, dsn, name }) => {
            if let Some(current_id) = state.runtime.active_connection_id.clone() {
                let cache = save_current_cache(state);
                state.connection_caches.save(&current_id, cache);
            }

            state.runtime.active_connection_id = Some(id.clone());
            state.runtime.dsn = Some(dsn.clone());
            state.runtime.active_connection_name = Some(name.clone());

            // Try to restore from cache
            if let Some(cached) = state.connection_caches.get(id).cloned() {
                restore_cache(state, &cached);
                state.runtime.connection_state = ConnectionState::Connected;
                state.cache.state = MetadataState::Loaded;
                Some(vec![Effect::ClearCompletionEngineCache])
            } else {
                // No cache: fetch metadata
                state.runtime.connection_state = ConnectionState::Connecting;
                state.cache.state = MetadataState::Loading;
                reset_connection_state(state);
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
    use crate::app::connection_cache::ConnectionCache;
    use crate::app::inspector_tab::InspectorTab;
    use crate::domain::ConnectionId;

    fn create_switch_action(id: &ConnectionId, name: &str) -> Action {
        Action::SwitchConnection(ConnectionTarget {
            id: id.clone(),
            dsn: format!("postgres://localhost/{}", name),
            name: name.to_string(),
        })
    }

    #[test]
    fn saves_current_cache_before_switching() {
        let mut state = AppState::new("test".to_string());
        let current_id = ConnectionId::new();
        let new_id = ConnectionId::new();

        state.runtime.active_connection_id = Some(current_id.clone());
        state.ui.explorer_selected = 5;
        state.ui.inspector_tab = InspectorTab::Indexes;

        let action = create_switch_action(&new_id, "new_db");
        reduce(&mut state, &action, Instant::now());

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
        reduce(&mut state, &action, Instant::now());

        assert_eq!(state.ui.explorer_selected, 42);
        assert_eq!(state.ui.inspector_tab, InspectorTab::ForeignKeys);
    }

    #[test]
    fn fetches_metadata_when_no_cache_exists() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        let action = create_switch_action(&new_id, "fresh_db");
        let effects = reduce(&mut state, &action, Instant::now()).unwrap();

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::FetchMetadata { .. }))
        );
        assert_eq!(state.runtime.connection_state, ConnectionState::Connecting);
    }

    #[test]
    fn updates_active_connection_fields() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        let action = create_switch_action(&new_id, "target_db");
        reduce(&mut state, &action, Instant::now());

        assert_eq!(state.runtime.active_connection_id, Some(new_id));
        assert_eq!(
            state.runtime.dsn,
            Some("postgres://localhost/target_db".to_string())
        );
        assert_eq!(
            state.runtime.active_connection_name,
            Some("target_db".to_string())
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
        reduce(&mut state, &action, Instant::now());

        assert_eq!(state.runtime.connection_state, ConnectionState::Connected);
    }

    #[test]
    fn resets_result_selection_when_restoring_cache() {
        let mut state = AppState::new("test".to_string());
        let target_id = ConnectionId::new();

        state
            .connection_caches
            .save(&target_id, ConnectionCache::default());
        state.ui.result_selection.enter_row(3);
        state.ui.result_selection.enter_cell(2);

        let action = create_switch_action(&target_id, "cached_db");
        reduce(&mut state, &action, Instant::now());

        assert_eq!(
            state.ui.result_selection.mode(),
            crate::app::ui_state::ResultNavMode::Scroll
        );
    }

    #[test]
    fn resets_result_selection_when_no_cache() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        state.ui.result_selection.enter_row(5);

        let action = create_switch_action(&new_id, "fresh_db");
        reduce(&mut state, &action, Instant::now());

        assert_eq!(
            state.ui.result_selection.mode(),
            crate::app::ui_state::ResultNavMode::Scroll
        );
    }

    #[test]
    fn clears_completion_cache_on_switch() {
        let mut state = AppState::new("test".to_string());
        let new_id = ConnectionId::new();

        let action = create_switch_action(&new_id, "any_db");
        let effects = reduce(&mut state, &action, Instant::now()).unwrap();

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
        );
    }
}
