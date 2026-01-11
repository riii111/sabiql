//! Connection sub-reducer: connection setup, error handling, lifecycle.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::connection_setup_state::{ConnectionField, CONNECTION_INPUT_VISIBLE_WIDTH};
use crate::app::connection_state::ConnectionState;
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::reducers::{insert_char_at_cursor, validate_all, validate_field};
use crate::app::state::AppState;
use crate::domain::connection::SslMode;
use crate::domain::MetadataState;

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

        // ===== Connection Modes =====
        Action::OpenConnectionSetup => {
            state.ui.input_mode = InputMode::ConnectionSetup;
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
        Action::RetryConnection => {
            if state.runtime.connection_state.is_connecting() || state.runtime.is_reconnecting {
                return Some(vec![]);
            }
            if let Some(dsn) = state.runtime.dsn.clone() {
                state.runtime.is_reconnecting = true;
                state.connection_error.is_retrying = true;
                state.runtime.connection_state = ConnectionState::Connecting;
                state.cache.state = MetadataState::Loading;
                Some(vec![Effect::FetchMetadata { dsn }])
            } else {
                state.runtime.connection_state = ConnectionState::NotConnected;
                state.cache.state = MetadataState::NotLoaded;
                state.ui.input_mode = InputMode::ConnectionSetup;
                Some(vec![])
            }
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
        Action::ConnectionSaveCompleted { dsn } => {
            state.connection_setup.is_first_run = false;
            state.runtime.dsn = Some(dsn.clone());
            state.runtime.active_connection_name = Some(state.connection_setup.auto_name());
            state.runtime.connection_state = ConnectionState::Connecting;
            state.cache.state = MetadataState::Loading;
            state.ui.input_mode = InputMode::Normal;
            Some(vec![Effect::FetchMetadata { dsn: dsn.clone() }])
        }
        Action::ConnectionSaveFailed(msg) => {
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }

        _ => None,
    }
}
