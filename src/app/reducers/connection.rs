//! Connection sub-reducer: connection setup, error handling, lifecycle.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::connection_cache::ConnectionCache;
use crate::app::connection_setup_state::{CONNECTION_INPUT_VISIBLE_WIDTH, ConnectionField};
use crate::app::connection_state::ConnectionState;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::reducers::{insert_char_at_cursor, validate_all, validate_field};
use crate::app::state::AppState;
use crate::domain::MetadataState;
use crate::domain::connection::SslMode;

/// Handles connection lifecycle, setup form, and error handling.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_connection(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        // ===== Connection Lifecycle =====
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

        Action::SwitchConnection { id, dsn, name } => {
            // Save current connection's state
            if let Some(current_id) = state.runtime.active_connection_id.clone() {
                let cache = save_current_cache(state);
                state.connection_caches.save(&current_id, cache);
            }

            // Update active connection
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

        // ===== Connection Modes =====
        Action::OpenConnectionSelector => {
            state.ui.input_mode = InputMode::ConnectionSelector;
            Some(vec![Effect::LoadConnections])
        }
        Action::OpenConnectionSetup => {
            state.connection_setup.reset();
            state.ui.input_mode = InputMode::ConnectionSetup;
            Some(vec![])
        }
        Action::StartEditConnection(id) => {
            Some(vec![Effect::LoadConnectionForEdit { id: id.clone() }])
        }
        Action::ConnectionEditLoaded(profile) => {
            state.connection_setup =
                crate::app::connection_setup_state::ConnectionSetupState::from_profile(profile);
            state.ui.input_mode = InputMode::ConnectionSetup;
            Some(vec![])
        }
        Action::ConnectionEditLoadFailed(msg) => {
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }
        Action::CloseConnectionSetup => {
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ShowConnectionError(info) => {
            state.connection_error.set_error(info.clone());
            state.ui.input_mode = InputMode::ConnectionError;
            Some(vec![])
        }
        Action::CloseConnectionError => {
            state.connection_error.details_expanded = false;
            state.connection_error.scroll_offset = 0;
            state.connection_error.clear_copied_feedback();
            state.ui.input_mode = InputMode::Normal;
            Some(vec![])
        }
        Action::ToggleConnectionErrorDetails => {
            state.connection_error.toggle_details();
            Some(vec![])
        }
        Action::ScrollConnectionErrorUp => {
            state.connection_error.scroll_up();
            Some(vec![])
        }
        Action::ScrollConnectionErrorDown => {
            state.connection_error.scroll_down(100);
            Some(vec![])
        }
        Action::CopyConnectionError => {
            if let Some(content) = state.connection_error.masked_details() {
                Some(vec![Effect::CopyToClipboard {
                    content: content.to_string(),
                }])
            } else {
                Some(vec![])
            }
        }
        Action::ConnectionErrorCopied => {
            state.connection_error.mark_copied_at(now);
            Some(vec![])
        }
        Action::ReenterConnectionSetup => {
            state.connection_error.clear();
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.cache.state = MetadataState::NotLoaded;
            state.ui.input_mode = InputMode::ConnectionSetup;
            Some(vec![])
        }

        // ===== Connection Setup Form =====
        Action::ConnectionSetupInput(c) => {
            let setup = &mut state.connection_setup;
            match setup.focused_field {
                ConnectionField::Name => {
                    insert_char_at_cursor(&mut setup.name, setup.cursor_position, *c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::Host => {
                    insert_char_at_cursor(&mut setup.host, setup.cursor_position, *c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::Port => {
                    if c.is_ascii_digit() && setup.port.chars().count() < 5 {
                        insert_char_at_cursor(&mut setup.port, setup.cursor_position, *c);
                        let new_cursor = setup.cursor_position + 1;
                        setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
                ConnectionField::Database => {
                    insert_char_at_cursor(&mut setup.database, setup.cursor_position, *c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::User => {
                    insert_char_at_cursor(&mut setup.user, setup.cursor_position, *c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::Password => {
                    insert_char_at_cursor(&mut setup.password, setup.cursor_position, *c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::SslMode => {}
            }
            Some(vec![])
        }
        Action::ConnectionSetupBackspace => {
            let setup = &mut state.connection_setup;
            if setup.cursor_position == 0 {
                return Some(vec![]);
            }
            let field_str = match setup.focused_field {
                ConnectionField::Name => &mut setup.name,
                ConnectionField::Host => &mut setup.host,
                ConnectionField::Port => &mut setup.port,
                ConnectionField::Database => &mut setup.database,
                ConnectionField::User => &mut setup.user,
                ConnectionField::Password => &mut setup.password,
                ConnectionField::SslMode => return Some(vec![]),
            };
            let char_pos = setup.cursor_position - 1;
            if let Some((byte_idx, _)) = field_str.char_indices().nth(char_pos) {
                field_str.remove(byte_idx);
                setup.update_cursor(char_pos, CONNECTION_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::ConnectionSetupNextField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field);
            if let Some(next) = setup.focused_field.next() {
                setup.focused_field = next;
                setup.cursor_to_end();
            }
            Some(vec![])
        }
        Action::ConnectionSetupPrevField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field);
            if let Some(prev) = setup.focused_field.prev() {
                setup.focused_field = prev;
                setup.cursor_to_end();
            }
            Some(vec![])
        }
        Action::ConnectionSetupToggleDropdown => {
            let setup = &mut state.connection_setup;
            if setup.focused_field == ConnectionField::SslMode {
                setup.ssl_dropdown.is_open = !setup.ssl_dropdown.is_open;
                if setup.ssl_dropdown.is_open {
                    setup.ssl_dropdown.selected_index = SslMode::all_variants()
                        .iter()
                        .position(|v| *v == setup.ssl_mode)
                        .unwrap_or(2);
                }
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownNext => {
            let setup = &mut state.connection_setup;
            if setup.ssl_dropdown.is_open {
                let max = SslMode::all_variants().len() - 1;
                if setup.ssl_dropdown.selected_index < max {
                    setup.ssl_dropdown.selected_index += 1;
                }
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownPrev => {
            let setup = &mut state.connection_setup;
            if setup.ssl_dropdown.is_open {
                setup.ssl_dropdown.selected_index =
                    setup.ssl_dropdown.selected_index.saturating_sub(1);
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownConfirm => {
            let setup = &mut state.connection_setup;
            if setup.ssl_dropdown.is_open {
                if let Some(mode) = SslMode::all_variants().get(setup.ssl_dropdown.selected_index) {
                    setup.ssl_mode = *mode;
                }
                setup.ssl_dropdown.is_open = false;
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownCancel => {
            state.connection_setup.ssl_dropdown.is_open = false;
            Some(vec![])
        }
        Action::ConnectionSetupSave => {
            let setup = &mut state.connection_setup;
            validate_all(setup);
            if setup.validation_errors.is_empty() {
                let port = setup.port.parse().unwrap_or(5432);
                Some(vec![Effect::SaveAndConnect {
                    id: setup.editing_id.clone(),
                    name: setup.name.clone(),
                    host: setup.host.clone(),
                    port,
                    database: setup.database.clone(),
                    user: setup.user.clone(),
                    password: setup.password.clone(),
                    ssl_mode: setup.ssl_mode,
                }])
            } else {
                Some(vec![])
            }
        }
        Action::ConnectionSetupCancel => {
            if state.connection_setup.is_first_run {
                state.confirm_dialog.title = "Confirm".to_string();
                state.confirm_dialog.message =
                    "No connection configured.\nAre you sure you want to quit?".to_string();
                state.confirm_dialog.on_confirm = Action::Quit;
                state.confirm_dialog.on_cancel = Action::OpenConnectionSetup;
                state.ui.input_mode = InputMode::ConfirmDialog;
                Some(vec![])
            } else {
                state.ui.input_mode = InputMode::Normal;
                Some(vec![Effect::DispatchActions(vec![Action::TryConnect])])
            }
        }
        Action::ConnectionSaveCompleted {
            id,
            dsn,
            name,
            is_edit,
        } => {
            state.connection_setup.is_first_run = false;
            state.ui.input_mode = InputMode::Normal;

            if *is_edit {
                Some(vec![])
            } else {
                state.runtime.active_connection_id = Some(id.clone());
                state.runtime.dsn = Some(dsn.clone());
                state.runtime.active_connection_name = Some(name.clone());
                state.runtime.connection_state = ConnectionState::Connecting;
                state.cache.state = MetadataState::Loading;
                Some(vec![Effect::FetchMetadata { dsn: dsn.clone() }])
            }
        }
        Action::ConnectionSaveFailed(msg) => {
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }

        _ => None,
    }
}

fn save_current_cache(state: &AppState) -> ConnectionCache {
    ConnectionCache {
        metadata: state.cache.metadata.clone(),
        table_detail: state.cache.table_detail.clone(),
        current_table: state.cache.current_table.clone(),
        query_result: state.query.current_result.clone(),
        result_history: state.query.result_history.clone(),
        explorer_selected: state.ui.explorer_selected,
        inspector_tab: state.ui.inspector_tab,
    }
}

fn restore_cache(state: &mut AppState, cache: &ConnectionCache) {
    state.cache.metadata = cache.metadata.clone();
    state.cache.table_detail = cache.table_detail.clone();
    state.cache.current_table = cache.current_table.clone();
    state.query.current_result = cache.query_result.clone();
    state.query.result_history = cache.result_history.clone();
    state.ui.explorer_selected = cache.explorer_selected;
    state.ui.inspector_tab = cache.inspector_tab;
    state
        .ui
        .set_explorer_selection(Some(cache.explorer_selected));
}

fn reset_connection_state(state: &mut AppState) {
    state.cache.metadata = None;
    state.cache.table_detail = None;
    state.cache.current_table = None;
    state.query.current_result = None;
    state.query.result_history = Default::default();
    state.ui.set_explorer_selection(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::action::Action;
    use crate::app::effect::Effect;

    mod open_connection_selector {
        use super::*;

        #[test]
        fn sets_mode_and_loads_connections() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::Normal;

            let effects =
                reduce_connection(&mut state, &Action::OpenConnectionSelector, Instant::now());

            assert_eq!(state.ui.input_mode, InputMode::ConnectionSelector);
            let effects = effects.unwrap();
            assert!(effects.iter().any(|e| matches!(e, Effect::LoadConnections)));
        }
    }
}
