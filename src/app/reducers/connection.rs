//! Connection sub-reducer: connection setup, error handling, lifecycle.

use std::time::Instant;

use crate::app::action::{Action, CursorMove};
use crate::app::connection_cache::ConnectionCache;
use crate::app::connection_setup_state::{CONNECTION_INPUT_VISIBLE_WIDTH, ConnectionField};
use crate::app::connection_state::ConnectionState;
use crate::app::effect::Effect;
use crate::app::explorer_mode::ExplorerMode;
use crate::app::input_mode::InputMode;
use crate::app::reducers::{
    insert_char_at_cursor, insert_str_at_cursor, validate_all, validate_field,
};
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
            if !state.connections.is_empty() || state.runtime.dsn.is_some() {
                state.connection_setup.is_first_run = false;
            }
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
                    on_success: Some(Action::ConnectionErrorCopied),
                    on_failure: None,
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

        // ===== Clipboard Paste =====
        Action::Paste(text) if state.ui.input_mode == InputMode::ConnectionSetup => {
            let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            let setup = &mut state.connection_setup;
            match setup.focused_field {
                ConnectionField::Port => {
                    let current_len = setup.port.chars().count();
                    let remaining = 5usize.saturating_sub(current_len);
                    let digits: String = clean
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .take(remaining)
                        .collect();
                    if !digits.is_empty() {
                        let inserted =
                            insert_str_at_cursor(&mut setup.port, setup.cursor_position, &digits);
                        let new_cursor = setup.cursor_position + inserted;
                        setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
                ConnectionField::SslMode => {}
                _ => {
                    let field_str = match setup.focused_field {
                        ConnectionField::Name => &mut setup.name,
                        ConnectionField::Host => &mut setup.host,
                        ConnectionField::Database => &mut setup.database,
                        ConnectionField::User => &mut setup.user,
                        ConnectionField::Password => &mut setup.password,
                        _ => unreachable!(),
                    };
                    let inserted = insert_str_at_cursor(field_str, setup.cursor_position, &clean);
                    let new_cursor = setup.cursor_position + inserted;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
            }
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
        Action::ConnectionSetupMoveCursor(movement) => {
            let setup = &mut state.connection_setup;
            let field_str = match setup.focused_field {
                ConnectionField::Name => &setup.name,
                ConnectionField::Host => &setup.host,
                ConnectionField::Port => &setup.port,
                ConnectionField::Database => &setup.database,
                ConnectionField::User => &setup.user,
                ConnectionField::Password => &setup.password,
                ConnectionField::SslMode => return Some(vec![]),
            };
            let len = field_str.chars().count();
            let new_pos = match movement {
                CursorMove::Left => setup.cursor_position.saturating_sub(1),
                CursorMove::Right => (setup.cursor_position + 1).min(len),
                CursorMove::Home => 0,
                CursorMove::End => len,
                CursorMove::Up | CursorMove::Down => return Some(vec![]),
            };
            setup.update_cursor(new_pos, CONNECTION_INPUT_VISIBLE_WIDTH);
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
                state.runtime.connection_state = ConnectionState::Connecting;
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
        Action::ConnectionSaveCompleted { id, dsn, name } => {
            state.connection_setup.is_first_run = false;
            state.ui.input_mode = InputMode::Normal;
            state.ui.explorer_mode = ExplorerMode::Tables;
            state.runtime.active_connection_id = Some(id.clone());
            state.runtime.dsn = Some(dsn.clone());
            state.runtime.active_connection_name = Some(name.clone());
            state.runtime.connection_state = ConnectionState::Connecting;
            state.cache.state = MetadataState::Loading;
            Some(vec![Effect::FetchMetadata { dsn: dsn.clone() }])
        }
        Action::ConnectionSaveFailed(msg) => {
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.cache.state = MetadataState::NotLoaded;
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }

        // ===== Connection Deletion =====
        Action::RequestDeleteSelectedConnection => {
            let selected_idx = state.ui.connection_list_selected;
            if let Some(connection) = state.connections.get(selected_idx) {
                let id = connection.id.clone();
                let name = connection.name.as_str().to_string();
                let is_active = state.runtime.active_connection_id.as_ref() == Some(&id);

                state.confirm_dialog.title = "Delete Connection".to_string();

                if is_active {
                    state.confirm_dialog.message = format!(
                        "Delete \"{}\"?\n\n\u{26A0} This is the active connection.\nYou will be disconnected.\n\nThis action cannot be undone.",
                        name
                    );
                } else {
                    state.confirm_dialog.message =
                        format!("Delete \"{}\"?\n\nThis action cannot be undone.", name);
                }

                state.confirm_dialog.on_confirm = Action::DeleteConnection(id);
                state.confirm_dialog.on_cancel = Action::None;
                state.confirm_dialog.return_mode = state.ui.input_mode;
                state.ui.input_mode = InputMode::ConfirmDialog;
            }
            Some(vec![])
        }
        Action::DeleteConnection(id) => Some(vec![Effect::DeleteConnection { id: id.clone() }]),
        Action::ConnectionDeleted(id) => {
            if state.runtime.active_connection_id.as_ref() == Some(id) {
                state.runtime.active_connection_id = None;
                state.runtime.dsn = None;
                state.runtime.active_connection_name = None;
                state.runtime.connection_state = ConnectionState::NotConnected;
                state.cache.state = MetadataState::NotLoaded;
                state.cache.metadata = None;
                state.cache.table_detail = None;
                state.cache.current_table = None;
                state.query.current_result = None;
                state.query.result_history = Default::default();
                state.ui.set_explorer_selection(None);
            }

            state.connections.retain(|c| &c.id != id);
            state.connection_caches.remove(id);

            let len = state.connections.len();
            if state.ui.connection_list_selected >= len && len > 0 {
                state.ui.connection_list_selected = len - 1;
                state.ui.connection_list_state.select(Some(len - 1));
            }

            if state.connections.is_empty() {
                state.connection_setup.reset();
                state.connection_setup.is_first_run = false;
                state.ui.input_mode = InputMode::ConnectionSetup;
            }

            state
                .messages
                .set_success_at("Connection deleted".to_string(), now);
            Some(vec![])
        }
        Action::ConnectionDeleteFailed(msg) => {
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }

        // ===== Connection Edit =====
        Action::RequestEditSelectedConnection => {
            let selected_idx = state.ui.connection_list_selected;
            if let Some(connection) = state.connections.get(selected_idx) {
                let id = connection.id.clone();
                Some(vec![Effect::LoadConnectionForEdit { id }])
            } else {
                Some(vec![])
            }
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
    state.ui.result_selection.reset();
    state.ui.result_scroll_offset = 0;
    state.ui.result_horizontal_offset = 0;
    state.cell_edit.clear();
    state.pending_write_preview = None;
}

fn reset_connection_state(state: &mut AppState) {
    state.cache.metadata = None;
    state.cache.table_detail = None;
    state.cache.current_table = None;
    state.query.current_result = None;
    state.query.result_history = Default::default();
    state.ui.set_explorer_selection(None);
    state.ui.result_selection.reset();
    state.ui.result_scroll_offset = 0;
    state.ui.result_horizontal_offset = 0;
    state.cell_edit.clear();
    state.pending_write_preview = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::action::Action;
    use crate::app::effect::Effect;
    use crate::domain::connection::ConnectionProfile;

    fn create_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile::new(
            name.to_string(),
            "localhost".to_string(),
            5432,
            "db".to_string(),
            "user".to_string(),
            "pass".to_string(),
            Default::default(),
        )
        .unwrap()
    }

    mod paste {
        use super::*;
        use crate::app::connection_setup_state::ConnectionField;

        fn setup_state_with_field(field: ConnectionField) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::ConnectionSetup;
            state.connection_setup.focused_field = field;
            state.connection_setup.cursor_position = 0;
            // Clear default values so tests start clean
            state.connection_setup.host.clear();
            state.connection_setup.port.clear();
            state.connection_setup.database.clear();
            state.connection_setup.user.clear();
            state.connection_setup.name.clear();
            state.connection_setup.password.clear();
            state
        }

        #[test]
        fn paste_into_host_inserts_text() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce_connection(
                &mut state,
                &Action::Paste("db.example.com".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.host, "db.example.com");
        }

        #[test]
        fn paste_into_port_filters_non_digits() {
            let mut state = setup_state_with_field(ConnectionField::Port);

            reduce_connection(
                &mut state,
                &Action::Paste("54ab32".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.port, "5432");
        }

        #[test]
        fn paste_into_port_respects_limit() {
            let mut state = setup_state_with_field(ConnectionField::Port);
            state.connection_setup.port = "54".to_string();
            state.connection_setup.cursor_position = 2;

            reduce_connection(
                &mut state,
                &Action::Paste("321000".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.port, "54321");
        }

        #[test]
        fn paste_into_full_port_does_nothing() {
            let mut state = setup_state_with_field(ConnectionField::Port);
            state.connection_setup.port = "12345".to_string();
            state.connection_setup.cursor_position = 5;

            reduce_connection(&mut state, &Action::Paste("6".to_string()), Instant::now());

            assert_eq!(state.connection_setup.port, "12345");
        }

        #[test]
        fn paste_strips_newlines() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce_connection(
                &mut state,
                &Action::Paste("local\nhost".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.host, "localhost");
        }

        #[test]
        fn paste_into_ssl_mode_ignored() {
            let mut state = setup_state_with_field(ConnectionField::SslMode);
            let ssl_mode_before = state.connection_setup.ssl_mode;

            reduce_connection(
                &mut state,
                &Action::Paste("disable".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.ssl_mode, ssl_mode_before);
        }

        #[test]
        fn paste_updates_cursor_and_viewport() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce_connection(
                &mut state,
                &Action::Paste("db.example.com".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.cursor_position, 14);
        }
    }

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

    mod open_connection_setup {
        use super::*;

        #[test]
        fn is_first_run_true_when_no_connections() {
            let mut state = AppState::new("test".to_string());

            reduce_connection(&mut state, &Action::OpenConnectionSetup, Instant::now());

            assert!(state.connection_setup.is_first_run);
        }

        #[test]
        fn is_first_run_false_when_connections_exist() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("test");
            state.connections = vec![profile];

            reduce_connection(&mut state, &Action::OpenConnectionSetup, Instant::now());

            assert!(!state.connection_setup.is_first_run);
        }

        #[test]
        fn is_first_run_false_when_already_connected() {
            let mut state = AppState::new("test".to_string());
            state.runtime.dsn = Some("postgres://localhost/db".to_string());

            reduce_connection(&mut state, &Action::OpenConnectionSetup, Instant::now());

            assert!(!state.connection_setup.is_first_run);
        }
    }

    mod request_delete_selected_connection {
        use super::*;

        #[test]
        fn opens_confirm_dialog_with_correct_message() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            state.connections = vec![profile];
            state.ui.connection_list_selected = 0;

            reduce_connection(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert_eq!(state.ui.input_mode, InputMode::ConfirmDialog);
            assert_eq!(state.confirm_dialog.title, "Delete Connection");
            assert!(state.confirm_dialog.message.contains("Production"));
            assert!(
                state
                    .confirm_dialog
                    .message
                    .contains("This action cannot be undone")
            );
        }

        #[test]
        fn active_connection_shows_warning() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            let profile_id = profile.id.clone();
            state.connections = vec![profile];
            state.ui.connection_list_selected = 0;
            state.runtime.active_connection_id = Some(profile_id);

            reduce_connection(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert!(
                state
                    .confirm_dialog
                    .message
                    .contains("This is the active connection")
            );
            assert!(
                state
                    .confirm_dialog
                    .message
                    .contains("You will be disconnected")
            );
        }

        #[test]
        fn inactive_connection_shows_standard_message() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            state.connections = vec![profile];
            state.ui.connection_list_selected = 0;
            // No active connection set

            reduce_connection(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert!(
                !state
                    .confirm_dialog
                    .message
                    .contains("This is the active connection")
            );
        }

        #[test]
        fn empty_list_does_nothing() {
            let mut state = AppState::new("test".to_string());
            state.connections = vec![];
            state.ui.input_mode = InputMode::Normal;

            reduce_connection(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert_eq!(state.ui.input_mode, InputMode::Normal);
        }

        #[test]
        fn preserves_return_mode_from_connection_selector() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            state.connections = vec![profile];
            state.ui.connection_list_selected = 0;
            state.ui.input_mode = InputMode::ConnectionSelector;

            reduce_connection(
                &mut state,
                &Action::RequestDeleteSelectedConnection,
                Instant::now(),
            );

            assert_eq!(
                state.confirm_dialog.return_mode,
                InputMode::ConnectionSelector
            );
        }
    }

    mod connection_deleted {
        use super::*;

        #[test]
        fn removes_connection_from_list() {
            let mut state = AppState::new("test".to_string());
            let profile1 = create_profile("First");
            let profile2 = create_profile("Second");
            let id_to_delete = profile1.id.clone();
            state.connections = vec![profile1, profile2];

            reduce_connection(
                &mut state,
                &Action::ConnectionDeleted(id_to_delete),
                Instant::now(),
            );

            assert_eq!(state.connections.len(), 1);
            assert_eq!(state.connections[0].name.as_str(), "Second");
        }

        #[test]
        fn clears_active_state_when_active_deleted() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("Production");
            let profile_id = profile.id.clone();
            state.connections = vec![profile];
            state.runtime.active_connection_id = Some(profile_id.clone());
            state.runtime.dsn = Some("postgres://localhost/db".to_string());
            state.runtime.connection_state = ConnectionState::Connected;

            reduce_connection(
                &mut state,
                &Action::ConnectionDeleted(profile_id),
                Instant::now(),
            );

            assert!(state.runtime.active_connection_id.is_none());
            assert!(state.runtime.dsn.is_none());
            assert!(state.runtime.connection_state.is_not_connected());
        }

        #[test]
        fn adjusts_selection_when_last_item_deleted() {
            let mut state = AppState::new("test".to_string());
            let profile1 = create_profile("First");
            let profile2 = create_profile("Second");
            let id_to_delete = profile2.id.clone();
            state.connections = vec![profile1, profile2];
            state.ui.connection_list_selected = 1; // Select last item

            reduce_connection(
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
            state.connections = vec![profile];
            state.ui.input_mode = InputMode::Normal;

            reduce_connection(
                &mut state,
                &Action::ConnectionDeleted(profile_id),
                Instant::now(),
            );

            assert!(state.connections.is_empty());
            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
        }
    }

    mod switch_connection {
        use super::*;
        use crate::app::connection_cache::ConnectionCache;
        use crate::app::inspector_tab::InspectorTab;
        use crate::domain::ConnectionId;

        fn create_switch_action(id: &ConnectionId, name: &str) -> Action {
            Action::SwitchConnection {
                id: id.clone(),
                dsn: format!("postgres://localhost/{}", name),
                name: name.to_string(),
            }
        }

        #[test]
        fn saves_current_cache_before_switching() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            let new_id = ConnectionId::new();

            // Set up current connection with some state
            state.runtime.active_connection_id = Some(current_id.clone());
            state.ui.explorer_selected = 5;
            state.ui.inspector_tab = InspectorTab::Indexes;

            let action = create_switch_action(&new_id, "new_db");
            reduce_connection(&mut state, &action, Instant::now());

            // Verify current state was saved
            let saved = state.connection_caches.get(&current_id).unwrap();
            assert_eq!(saved.explorer_selected, 5);
            assert_eq!(saved.inspector_tab, InspectorTab::Indexes);
        }

        #[test]
        fn restores_cached_state_when_available() {
            let mut state = AppState::new("test".to_string());
            let target_id = ConnectionId::new();

            // Pre-populate cache for target connection
            let cached = ConnectionCache {
                explorer_selected: 42,
                inspector_tab: InspectorTab::ForeignKeys,
                ..Default::default()
            };
            state.connection_caches.save(&target_id, cached);

            let action = create_switch_action(&target_id, "cached_db");
            reduce_connection(&mut state, &action, Instant::now());

            assert_eq!(state.ui.explorer_selected, 42);
            assert_eq!(state.ui.inspector_tab, InspectorTab::ForeignKeys);
        }

        #[test]
        fn fetches_metadata_when_no_cache_exists() {
            let mut state = AppState::new("test".to_string());
            let new_id = ConnectionId::new();

            let action = create_switch_action(&new_id, "fresh_db");
            let effects = reduce_connection(&mut state, &action, Instant::now()).unwrap();

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
            reduce_connection(&mut state, &action, Instant::now());

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

            // Pre-populate cache
            state
                .connection_caches
                .save(&target_id, ConnectionCache::default());

            let action = create_switch_action(&target_id, "cached_db");
            reduce_connection(&mut state, &action, Instant::now());

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
            reduce_connection(&mut state, &action, Instant::now());

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
            reduce_connection(&mut state, &action, Instant::now());

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
            let effects = reduce_connection(&mut state, &action, Instant::now()).unwrap();

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ClearCompletionEngineCache))
            );
        }
    }
}
