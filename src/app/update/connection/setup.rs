use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::connection::{DatabaseType, SslMode};
use crate::model::app_state::AppState;
use crate::model::connection::setup::{
    CONNECTION_INPUT_VISIBLE_WIDTH, ConnectionField, ConnectionSetupState,
};
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{Action, ConnectionTarget, InputTarget, ModalKind};
use crate::update::helpers::{validate_all, validate_field};

pub fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
    match action {
        Action::OpenModal(ModalKind::ConnectionSetup) => {
            state.connection_setup.reset();
            if !state.connections().is_empty() || state.session.dsn.is_some() {
                state.connection_setup.is_first_run = false;
            }
            state.modal.set_mode(InputMode::ConnectionSetup);
            Some(vec![])
        }
        Action::StartEditConnection(id) => {
            Some(vec![Effect::LoadConnectionForEdit { id: id.clone() }])
        }
        Action::ConnectionEditLoaded(profile) => {
            state.connection_setup = ConnectionSetupState::from(&**profile);
            state.modal.set_mode(InputMode::ConnectionSetup);
            Some(vec![])
        }
        Action::ConnectionEditLoadFailed(e) => {
            state.messages.set_error_at(e.to_string(), now);
            Some(vec![])
        }
        Action::CloseModal(ModalKind::ConnectionSetup) => {
            state.modal.set_mode(InputMode::Normal);
            Some(vec![])
        }

        // ===== Clipboard Paste =====
        Action::Paste(text) if state.modal.active_mode() == InputMode::ConnectionSetup => {
            let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            let setup = &mut state.connection_setup;
            match setup.focused_field {
                ConnectionField::Port => {
                    let current_len = setup.port.char_count();
                    let remaining = 5usize.saturating_sub(current_len);
                    let digits: String = clean
                        .chars()
                        .filter(char::is_ascii_digit)
                        .take(remaining)
                        .collect();
                    if !digits.is_empty() {
                        setup.port.insert_str(&digits);
                        setup.port.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
                ConnectionField::DatabaseType | ConnectionField::SslMode => {}
                _ => {
                    if let Some(input) = setup.focused_input_mut() {
                        input.insert_str(&clean);
                        input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
            }
            Some(vec![])
        }

        // ===== Connection Setup Form =====
        Action::TextInput {
            target: InputTarget::ConnectionSetup,
            ch: c,
        } => {
            let setup = &mut state.connection_setup;
            match setup.focused_field {
                ConnectionField::Port => {
                    if c.is_ascii_digit() && setup.port.char_count() < 5 {
                        setup.port.insert_char(*c);
                        setup.port.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
                ConnectionField::DatabaseType | ConnectionField::SslMode => {}
                _ => {
                    if let Some(input) = setup.focused_input_mut() {
                        input.insert_char(*c);
                        input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
            }
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::ConnectionSetup,
        } => {
            let setup = &mut state.connection_setup;
            if let Some(input) = setup.focused_input_mut() {
                input.backspace();
                input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::TextMoveCursor {
            target: InputTarget::ConnectionSetup,
            direction: movement,
        } => {
            let setup = &mut state.connection_setup;
            if let Some(input) = setup.focused_input_mut() {
                input.move_cursor(*movement);
                input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
            }
            Some(vec![])
        }
        Action::ConnectionSetupNextField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field);
            if let Some(next) = setup.next_field() {
                setup.focused_field = next;
            }
            Some(vec![])
        }
        Action::ConnectionSetupPrevField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field);
            if let Some(prev) = setup.prev_field() {
                setup.focused_field = prev;
            }
            Some(vec![])
        }
        Action::ConnectionSetupToggleDropdown => {
            let setup = &mut state.connection_setup;
            match setup.focused_field {
                ConnectionField::DatabaseType => {
                    setup.database_type_dropdown.is_open = !setup.database_type_dropdown.is_open;
                    setup.ssl_dropdown.is_open = false;
                    if setup.database_type_dropdown.is_open {
                        setup.database_type_dropdown.selected_index = DatabaseType::all()
                            .iter()
                            .position(|v| *v == setup.database_type)
                            .unwrap_or(0);
                    }
                }
                ConnectionField::SslMode => {
                    setup.ssl_dropdown.is_open = !setup.ssl_dropdown.is_open;
                    setup.database_type_dropdown.is_open = false;
                    if setup.ssl_dropdown.is_open {
                        setup.ssl_dropdown.selected_index = SslMode::all_variants()
                            .iter()
                            .position(|v| *v == setup.ssl_mode)
                            .unwrap_or(2);
                    }
                }
                _ => {}
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownNext => {
            let setup = &mut state.connection_setup;
            if setup.database_type_dropdown.is_open {
                let max = DatabaseType::all().len() - 1;
                if setup.database_type_dropdown.selected_index < max {
                    setup.database_type_dropdown.selected_index += 1;
                }
            } else if setup.ssl_dropdown.is_open {
                let max = SslMode::all_variants().len() - 1;
                if setup.ssl_dropdown.selected_index < max {
                    setup.ssl_dropdown.selected_index += 1;
                }
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownPrev => {
            let setup = &mut state.connection_setup;
            if setup.database_type_dropdown.is_open {
                setup.database_type_dropdown.selected_index = setup
                    .database_type_dropdown
                    .selected_index
                    .saturating_sub(1);
            } else if setup.ssl_dropdown.is_open {
                setup.ssl_dropdown.selected_index =
                    setup.ssl_dropdown.selected_index.saturating_sub(1);
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownConfirm => {
            let setup = &mut state.connection_setup;
            if setup.database_type_dropdown.is_open {
                if let Some(database_type) =
                    DatabaseType::all().get(setup.database_type_dropdown.selected_index)
                {
                    setup.set_database_type(*database_type);
                }
            } else if setup.ssl_dropdown.is_open {
                if let Some(mode) = SslMode::all_variants().get(setup.ssl_dropdown.selected_index) {
                    setup.ssl_mode = *mode;
                }
                setup.ssl_dropdown.is_open = false;
            }
            Some(vec![])
        }
        Action::ConnectionSetupDropdownCancel => {
            state.connection_setup.database_type_dropdown.is_open = false;
            state.connection_setup.ssl_dropdown.is_open = false;
            Some(vec![])
        }
        Action::ConnectionSetupSave => {
            let setup = &mut state.connection_setup;
            validate_all(setup);
            if setup.validation_errors.is_empty() {
                let config = match setup.to_connection_config() {
                    Ok(config) => config,
                    Err(crate::domain::connection::SqliteConnectionConfigError::EmptyPath) => {
                        setup
                            .validation_errors
                            .insert(ConnectionField::SqlitePath, "Required".to_string());
                        return Some(vec![]);
                    }
                    Err(
                        crate::domain::connection::SqliteConnectionConfigError::UnsupportedPath,
                    ) => {
                        setup.validation_errors.insert(
                            ConnectionField::SqlitePath,
                            "Unsupported characters".to_string(),
                        );
                        return Some(vec![]);
                    }
                };
                if config.database_type() != DatabaseType::SQLite {
                    state.session.mark_connecting();
                }
                Some(vec![Effect::SaveAndConnect {
                    id: setup.editing_id.clone(),
                    name: setup.name.content().to_string(),
                    config,
                }])
            } else {
                Some(vec![])
            }
        }
        Action::ConnectionSetupCancel => {
            if state.connection_setup.is_first_run {
                state.confirm_dialog.open(
                    "Confirm",
                    "No connection configured.\nAre you sure you want to quit?",
                    crate::model::shared::confirm_dialog::ConfirmIntent::QuitNoConnection,
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
                Some(vec![])
            } else {
                state.modal.set_mode(InputMode::Normal);
                Some(vec![Effect::DispatchActions(vec![Action::TryConnect])])
            }
        }
        Action::ConnectionSaveCompleted(ConnectionTarget {
            id,
            dsn,
            name,
            database_type,
        }) => {
            state.connection_setup.is_first_run = false;
            state.modal.set_mode(InputMode::Normal);
            state.session.active_connection_id = Some(id.clone());
            state.session.active_connection_name = Some(name.clone());
            state.session.read_only = false;
            if *database_type == DatabaseType::SQLite {
                state.session.dsn = Some(dsn.clone());
                state.session.mark_disconnected();
                return Some(vec![]);
            }
            state.session.begin_connecting(dsn);
            Some(vec![Effect::FetchMetadata { dsn: dsn.clone() }])
        }
        Action::ConnectionSaveFailed(e) => {
            if !state.session.connection_state().is_connected() {
                state.session.mark_disconnected();
            }
            state.messages.set_error_at(e.to_string(), now);
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::ConnectionProfile;
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

    mod paste {
        use super::*;
        use crate::model::connection::setup::ConnectionField;
        use crate::model::shared::text_input::TextInputState;

        fn setup_state_with_field(field: ConnectionField) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::ConnectionSetup);
            state.connection_setup.focused_field = field;
            // Clear default values so tests start clean
            state.connection_setup.host = TextInputState::default();
            state.connection_setup.port = TextInputState::default();
            state.connection_setup.database = TextInputState::default();
            state.connection_setup.user = TextInputState::default();
            state.connection_setup.name = TextInputState::default();
            state.connection_setup.password = TextInputState::default();
            state
        }

        #[test]
        fn host_inserts_text() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce(
                &mut state,
                &Action::Paste("db.example.com".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.host.content(), "db.example.com");
        }

        #[test]
        fn port_filters_non_digits() {
            let mut state = setup_state_with_field(ConnectionField::Port);

            reduce(
                &mut state,
                &Action::Paste("54ab32".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.port.content(), "5432");
        }

        #[test]
        fn port_respects_limit() {
            let mut state = setup_state_with_field(ConnectionField::Port);
            state.connection_setup.port.set_content("54".to_string());

            reduce(
                &mut state,
                &Action::Paste("321000".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.port.content(), "54321");
        }

        #[test]
        fn full_port_does_nothing() {
            let mut state = setup_state_with_field(ConnectionField::Port);
            state.connection_setup.port.set_content("12345".to_string());

            reduce(&mut state, &Action::Paste("6".to_string()), Instant::now());

            assert_eq!(state.connection_setup.port.content(), "12345");
        }

        #[test]
        fn strips_newlines() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce(
                &mut state,
                &Action::Paste("local\nhost".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.host.content(), "localhost");
        }

        #[test]
        fn ssl_mode_ignored() {
            let mut state = setup_state_with_field(ConnectionField::SslMode);
            let ssl_mode_before = state.connection_setup.ssl_mode;

            reduce(
                &mut state,
                &Action::Paste("disable".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.ssl_mode, ssl_mode_before);
        }

        #[test]
        fn updates_cursor() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce(
                &mut state,
                &Action::Paste("db.example.com".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.host.cursor(), 14);
        }
    }

    mod connection_save {
        use super::*;
        use crate::domain::MetadataState;
        use crate::model::connection::state::ConnectionState;
        use crate::update::action::ConnectionTarget;

        fn fill_valid_form(state: &mut AppState) {
            state.connection_setup.name.set_content("test".to_string());
            state
                .connection_setup
                .host
                .set_content("localhost".to_string());
            state.connection_setup.port.set_content("5432".to_string());
            state
                .connection_setup
                .database
                .set_content("db".to_string());
            state.connection_setup.user.set_content("user".to_string());
            state
                .connection_setup
                .password
                .set_content("pass".to_string());
        }

        #[test]
        fn save_sets_connection_and_metadata_state_as_pair() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);

            reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());

            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
            assert_eq!(state.session.metadata_state(), &MetadataState::Loading);
        }

        #[test]
        fn sqlite_save_does_not_enter_connecting_state() {
            let mut state = AppState::new("test".to_string());
            state.connection_setup.database_type = DatabaseType::SQLite;
            state.connection_setup.name.set_content("Local".to_string());
            state
                .connection_setup
                .sqlite_path
                .set_content("/tmp/app.db".to_string());

            let effects = reduce(&mut state, &Action::ConnectionSetupSave, Instant::now())
                .expect("save handled");

            assert_eq!(
                state.session.connection_state(),
                ConnectionState::NotConnected
            );
            assert_eq!(state.session.metadata_state(), &MetadataState::NotLoaded);
            assert!(matches!(
                effects.as_slice(),
                [Effect::SaveAndConnect { .. }]
            ));
        }

        #[test]
        fn save_completed_resets_read_only() {
            let mut state = AppState::new("test".to_string());
            state.session.read_only = true;

            let action = Action::ConnectionSaveCompleted(ConnectionTarget {
                id: crate::domain::ConnectionId::new(),
                dsn: "postgres://localhost/new_db".to_string(),
                name: "new_db".to_string(),
                database_type: DatabaseType::PostgreSQL,
            });
            reduce(&mut state, &action, Instant::now());

            assert!(!state.session.read_only);
        }

        #[test]
        fn sqlite_save_completed_does_not_fetch_metadata() {
            let mut state = AppState::new("test".to_string());

            let action = Action::ConnectionSaveCompleted(ConnectionTarget {
                id: crate::domain::ConnectionId::new(),
                dsn: "sqlite:///tmp/app.db".to_string(),
                name: "app.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            let effects = reduce(&mut state, &action, Instant::now()).unwrap();

            assert!(effects.is_empty());
            assert_eq!(state.session.dsn, Some("sqlite:///tmp/app.db".to_string()));
            assert!(state.session.connection_state().is_not_connected());
        }
    }

    mod open_connection_setup {
        use super::*;

        #[test]
        fn is_first_run_true_when_no_connections() {
            let mut state = AppState::new("test".to_string());

            reduce(
                &mut state,
                &Action::OpenModal(ModalKind::ConnectionSetup),
                Instant::now(),
            );

            assert!(state.connection_setup.is_first_run);
        }

        #[test]
        fn is_first_run_false_when_connections_exist() {
            let mut state = AppState::new("test".to_string());
            let profile = create_profile("test");
            state.set_connections(vec![profile]);

            reduce(
                &mut state,
                &Action::OpenModal(ModalKind::ConnectionSetup),
                Instant::now(),
            );

            assert!(!state.connection_setup.is_first_run);
        }

        #[test]
        fn is_first_run_false_when_already_connected() {
            let mut state = AppState::new("test".to_string());
            state.session.dsn = Some("postgres://localhost/db".to_string());

            reduce(
                &mut state,
                &Action::OpenModal(ModalKind::ConnectionSetup),
                Instant::now(),
            );

            assert!(!state.connection_setup.is_first_run);
        }
    }
}
