use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::connection::ConnectionProfileError;
use crate::model::app_state::AppState;
use crate::model::connection::setup::{
    CONNECTION_INPUT_VISIBLE_WIDTH, ConnectionField, ConnectionSetupState,
};
use crate::model::connection::state::ConnectionState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;
use crate::update::action::{
    Action, ConnectionSaveError, ConnectionTarget, InputTarget, ModalKind,
};
use crate::update::connection::helpers::{
    connection_save_fetch_effects, reset_for_new_connection, save_current_cache,
};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::{validate_all, validate_field};

pub fn reduce_connection_setup(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::ConnectionSetup) => {
            state.connection_setup.reset();
            if !state.connections().is_empty() || state.session.dsn().is_some() {
                state.connection_setup.set_first_run(false);
            }
            state.modal.set_mode(InputMode::ConnectionSetup);
            DispatchResult::handled()
        }
        Action::StartEditConnection(id) => {
            DispatchResult::handled_with(vec![Effect::LoadConnectionForEdit { id: id.clone() }])
        }
        Action::ConnectionEditLoaded(profile) => {
            state.connection_setup = ConnectionSetupState::from(&**profile);
            state.modal.set_mode(InputMode::ConnectionSetup);
            DispatchResult::handled()
        }
        Action::ConnectionEditLoadFailed(e) => {
            state.messages.set_error_at(e.to_string(), now);
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::ConnectionSetup) => {
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled()
        }

        // ===== Clipboard Paste =====
        Action::Paste(text) if state.modal.active_mode() == InputMode::ConnectionSetup => {
            let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            let setup = &mut state.connection_setup;
            match setup.focused_field() {
                ConnectionField::Port => {
                    let port = setup.port_mut();
                    let current_len = port.char_count();
                    let remaining = remaining_input_capacity(ConnectionField::Port, current_len);
                    let digits: String = clean
                        .chars()
                        .filter(char::is_ascii_digit)
                        .take(remaining)
                        .collect();
                    if !digits.is_empty() {
                        port.insert_str(&digits);
                        port.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
                ConnectionField::DatabaseType | ConnectionField::SslMode => {}
                _ => {
                    let field = setup.focused_field();
                    if let Some(input) = setup.focused_input_mut() {
                        let remaining = remaining_input_capacity(field, input.char_count());
                        let allowed = take_chars(&clean, remaining);
                        if !allowed.is_empty() {
                            input.insert_str(&allowed);
                            input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                        }
                    }
                }
            }
            DispatchResult::handled()
        }

        // ===== Connection Setup Form =====
        Action::TextInput {
            target: InputTarget::ConnectionSetup,
            ch: c,
        } => {
            let setup = &mut state.connection_setup;
            match setup.focused_field() {
                ConnectionField::Port => {
                    let port = setup.port_mut();
                    if c.is_ascii_digit()
                        && remaining_input_capacity(ConnectionField::Port, port.char_count()) > 0
                    {
                        port.insert_char(*c);
                        port.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
                ConnectionField::DatabaseType | ConnectionField::SslMode => {}
                _ => {
                    let field = setup.focused_field();
                    if let Some(input) = setup.focused_input_mut()
                        && remaining_input_capacity(field, input.char_count()) > 0
                    {
                        input.insert_char(*c);
                        input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
            }
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::ConnectionSetup,
        } => {
            let setup = &mut state.connection_setup;
            if let Some(input) = setup.focused_input_mut() {
                input.backspace();
                input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
            }
            DispatchResult::handled()
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
            DispatchResult::handled()
        }
        Action::ConnectionSetupNextField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field());
            setup.focus_next_field();
            DispatchResult::handled()
        }
        Action::ConnectionSetupPrevField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field());
            setup.focus_prev_field();
            DispatchResult::handled()
        }
        Action::ConnectionSetupToggleDropdown => {
            state.connection_setup.toggle_focused_dropdown();
            DispatchResult::handled()
        }
        Action::ConnectionSetupDropdownNext => {
            state.connection_setup.dropdown_next();
            DispatchResult::handled()
        }
        Action::ConnectionSetupDropdownPrev => {
            state.connection_setup.dropdown_prev();
            DispatchResult::handled()
        }
        Action::ConnectionSetupDropdownConfirm => {
            state.connection_setup.confirm_dropdown();
            DispatchResult::handled()
        }
        Action::ConnectionSetupDropdownCancel => {
            state.connection_setup.cancel_dropdown();
            DispatchResult::handled()
        }
        Action::ConnectionSetupSave => {
            state.connection_setup.confirm_dropdown();
            validate_all(&mut state.connection_setup);
            if state.connection_setup.has_validation_errors() {
                return DispatchResult::handled();
            }
            let config = match state.connection_setup.to_connection_config() {
                Ok(config) => config,
                Err(error) => {
                    state.connection_setup.record_sqlite_config_error(error);
                    return DispatchResult::handled();
                }
            };
            if state.session.connection_state() == ConnectionState::Connected
                && let Some(current_id) = state.session.active_connection_id().cloned()
            {
                let cache = save_current_cache(state);
                state.connection_caches.save(&current_id, cache);
            }
            state.query.mark_idle();
            state.session.mark_connecting();
            DispatchResult::handled_with(vec![Effect::SaveAndConnect {
                id: state.connection_setup.editing_id().cloned(),
                name: state
                    .connection_setup
                    .input(ConnectionField::Name)
                    .expect("name is a text input")
                    .content()
                    .trim()
                    .to_string(),
                config,
            }])
        }
        Action::ConnectionSetupCancel => {
            if state.connection_setup.is_first_run() {
                state.confirm_dialog.open(
                    "Confirm",
                    "No connection configured.\nAre you sure you want to quit?",
                    ConfirmIntent::QuitNoConnection,
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
                DispatchResult::handled()
            } else {
                state.modal.set_mode(InputMode::Normal);
                DispatchResult::handled_with(vec![Effect::DispatchActions(vec![
                    Action::TryConnect,
                ])])
            }
        }
        Action::ConnectionSaveCompleted(ConnectionTarget {
            id,
            dsn,
            name,
            database_type,
        }) => {
            state.connection_setup.set_first_run(false);
            state.modal.set_mode(InputMode::Normal);
            state.connection_caches.remove(id);

            reset_for_new_connection(state, id, dsn, name, *database_type);
            let run_id = state.session.begin_connecting(dsn);
            DispatchResult::handled_with(connection_save_fetch_effects(dsn, run_id, *database_type))
        }
        Action::ConnectionSaveFailed(e) => {
            if let ConnectionSaveError::Validation(ConnectionProfileError::SqlitePath(error)) = &e {
                state
                    .connection_setup
                    .record_sqlite_path_error(error.clone());
            }
            if !state.session.connection_state().is_connected() {
                state.session.mark_disconnected();
            }
            state.messages.set_error_at(e.to_string(), now);
            DispatchResult::handled()
        }

        _ => DispatchResult::pass(),
    }
}

fn remaining_input_capacity(field: ConnectionField, current_len: usize) -> usize {
    field
        .max_chars()
        .map_or(usize::MAX, |max| max.saturating_sub(current_len))
}

fn take_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::{ConnectionConfig, ConnectionProfile, SslMode};
    use crate::domain::{ConnectionId, DatabaseType};
    use crate::model::er_state::ErStatus;
    use crate::update::test_fixtures;
    fn reduce(state: &mut AppState, action: &Action, now: Instant) -> Option<Vec<Effect>> {
        reduce_connection_setup(state, action, now).into_effects()
    }

    fn create_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile::new_postgres(
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

        fn setup_state_with_field(field: ConnectionField) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::ConnectionSetup);
            for _ in 0..state.connection_setup.visible_fields().len() {
                if state.connection_setup.focused_field() == field {
                    break;
                }
                state.connection_setup.focus_next_field();
            }
            assert_eq!(state.connection_setup.focused_field(), field);
            // Clear default values so tests start clean
            for field in [
                ConnectionField::Host,
                ConnectionField::Port,
                ConnectionField::Database,
                ConnectionField::User,
                ConnectionField::Name,
                ConnectionField::Password,
            ] {
                state.connection_setup.input_mut(field).unwrap().clear();
            }
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

            assert_eq!(
                state
                    .connection_setup
                    .input(ConnectionField::Host)
                    .unwrap()
                    .content(),
                "db.example.com"
            );
        }

        #[test]
        fn port_filters_non_digits() {
            let mut state = setup_state_with_field(ConnectionField::Port);

            reduce(
                &mut state,
                &Action::Paste("54ab32".to_string()),
                Instant::now(),
            );

            assert_eq!(
                state
                    .connection_setup
                    .input(ConnectionField::Port)
                    .unwrap()
                    .content(),
                "5432"
            );
        }

        #[test]
        fn port_respects_limit() {
            let mut state = setup_state_with_field(ConnectionField::Port);
            state
                .connection_setup
                .input_mut(ConnectionField::Port)
                .unwrap()
                .set_content("54".to_string());

            reduce(
                &mut state,
                &Action::Paste("321000".to_string()),
                Instant::now(),
            );

            assert_eq!(
                state
                    .connection_setup
                    .input(ConnectionField::Port)
                    .unwrap()
                    .content(),
                "54321"
            );
        }

        #[test]
        fn full_port_does_nothing() {
            let mut state = setup_state_with_field(ConnectionField::Port);
            state
                .connection_setup
                .input_mut(ConnectionField::Port)
                .unwrap()
                .set_content("12345".to_string());

            reduce(&mut state, &Action::Paste("6".to_string()), Instant::now());

            assert_eq!(
                state
                    .connection_setup
                    .input(ConnectionField::Port)
                    .unwrap()
                    .content(),
                "12345"
            );
        }

        #[test]
        fn strips_newlines() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce(
                &mut state,
                &Action::Paste("local\nhost".to_string()),
                Instant::now(),
            );

            assert_eq!(
                state
                    .connection_setup
                    .input(ConnectionField::Host)
                    .unwrap()
                    .content(),
                "localhost"
            );
        }

        #[test]
        fn ssl_mode_ignored() {
            let mut state = setup_state_with_field(ConnectionField::SslMode);
            let ssl_mode_before = state.connection_setup.ssl_mode();

            reduce(
                &mut state,
                &Action::Paste("disable".to_string()),
                Instant::now(),
            );

            assert_eq!(state.connection_setup.ssl_mode(), ssl_mode_before);
        }

        #[test]
        fn updates_cursor() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce(
                &mut state,
                &Action::Paste("db.example.com".to_string()),
                Instant::now(),
            );

            assert_eq!(
                state
                    .connection_setup
                    .input(ConnectionField::Host)
                    .unwrap()
                    .cursor(),
                14
            );
        }

        #[test]
        fn host_paste_respects_limit() {
            let mut state = setup_state_with_field(ConnectionField::Host);

            reduce_connection_setup(&mut state, &Action::Paste("a".repeat(300)), Instant::now());

            assert_eq!(state.connection_setup.host.char_count(), 255);
        }
    }

    mod connection_save {
        use std::sync::Arc;

        use super::*;
        use crate::domain::{
            DatabaseMetadata, MetadataState, QueryResult, QuerySource, TableSummary,
        };
        use crate::model::connection::cache::ConnectionCache;
        use crate::model::connection::state::ConnectionState;
        use crate::update::action::ConnectionTarget;

        fn fill_valid_form(state: &mut AppState) {
            state
                .connection_setup
                .input_mut(ConnectionField::Name)
                .unwrap()
                .set_content("test".to_string());
            state
                .connection_setup
                .input_mut(ConnectionField::Host)
                .unwrap()
                .set_content("localhost".to_string());
            state
                .connection_setup
                .input_mut(ConnectionField::Port)
                .unwrap()
                .set_content("5432".to_string());
            state
                .connection_setup
                .input_mut(ConnectionField::Database)
                .unwrap()
                .set_content("db".to_string());
            state
                .connection_setup
                .input_mut(ConnectionField::User)
                .unwrap()
                .set_content("user".to_string());
            state
                .connection_setup
                .input_mut(ConnectionField::Password)
                .unwrap()
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
        fn save_terminates_active_query_run() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);
            let stale_run_id = state.query.begin_running(Instant::now());

            reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());

            assert!(!state.query.is_running());
            assert!(!state.query.is_current_run(stale_run_id));
        }

        #[test]
        fn sqlite_save_enters_connecting_state() {
            let mut state = AppState::new("test".to_string());
            state
                .connection_setup
                .set_database_type(DatabaseType::SQLite);
            state
                .connection_setup
                .input_mut(ConnectionField::Name)
                .unwrap()
                .set_content("Local".to_string());
            state
                .connection_setup
                .input_mut(ConnectionField::SqlitePath)
                .unwrap()
                .set_content("/tmp/app.db".to_string());

            let effects = reduce(&mut state, &Action::ConnectionSetupSave, Instant::now())
                .expect("save handled");

            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
            assert_eq!(state.session.metadata_state(), &MetadataState::Loading);
            assert!(matches!(
                effects.as_slice(),
                [Effect::SaveAndConnect { .. }]
            ));
        }

        #[test]
        fn save_confirms_open_ssl_dropdown_selection() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);
            state.connection_setup.ssl_mode = SslMode::Prefer;
            state.connection_setup.focused_field = ConnectionField::SslMode;
            state.connection_setup.toggle_focused_dropdown();
            while SslMode::all_variants()[state.connection_setup.ssl_dropdown().selected_index()]
                != SslMode::Require
            {
                state.connection_setup.dropdown_next();
            }

            let effects =
                reduce_connection_setup(&mut state, &Action::ConnectionSetupSave, Instant::now())
                    .unwrap();

            assert_eq!(state.connection_setup.ssl_mode, SslMode::Require);
            assert!(!state.connection_setup.ssl_dropdown().is_open());
            assert!(matches!(
                effects.as_slice(),
                [Effect::SaveAndConnect {
                    config: ConnectionConfig::PostgreSQL(config),
                    ..
                }] if config.ssl_mode == SslMode::Require
            ));
        }

        #[test]
        fn save_trims_connection_identifiers_and_name_but_preserves_password() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);
            state
                .connection_setup
                .name
                .set_content("  test-db  ".to_string());
            state
                .connection_setup
                .host
                .set_content("  localhost  ".to_string());
            state
                .connection_setup
                .database
                .set_content("  mydb  ".to_string());
            state
                .connection_setup
                .user
                .set_content("  postgres  ".to_string());
            state
                .connection_setup
                .password
                .set_content("  pass  ".to_string());

            let effects =
                reduce_connection_setup(&mut state, &Action::ConnectionSetupSave, Instant::now())
                    .unwrap();

            assert!(matches!(
                effects.as_slice(),
                [Effect::SaveAndConnect {
                    name,
                    config: ConnectionConfig::PostgreSQL(config),
                    ..
                }] if name == "test-db"
                    && config.host == "localhost"
                    && config.database == "mydb"
                    && config.username == "postgres"
                    && config.password == "  pass  "
            ));
        }

        #[test]
        fn save_completed_resets_read_only() {
            let mut state = AppState::new("test".to_string());
            state.session.enable_read_only();

            let action = Action::ConnectionSaveCompleted(ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "postgres://localhost/new_db".to_string(),
                name: "new_db".to_string(),
                database_type: DatabaseType::PostgreSQL,
            });
            reduce(&mut state, &action, Instant::now());

            assert!(!state.session.is_read_only());
        }

        #[test]
        fn save_completed_clears_previous_browse_state() {
            let mut state = AppState::new("test".to_string());
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/old");
            state.session.mark_connected(Arc::new({
                let mut metadata = DatabaseMetadata::new("old_db".to_string());
                metadata.table_summaries = vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    None,
                    false,
                )];
                metadata
            }));
            state.ui.set_explorer_selected_raw(3);
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);
            state
                .query
                .set_current_result(Arc::new(QueryResult::success(
                    "SELECT 1".to_string(),
                    vec!["col".to_string()],
                    vec![vec!["val".to_string()]],
                    10,
                    QuerySource::Preview,
                )));

            let action = Action::ConnectionSaveCompleted(ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "sqlite:///tmp/new.db".to_string(),
                name: "new.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            let effects = reduce(&mut state, &action, Instant::now()).unwrap();

            assert!(state.session.metadata().is_none());
            assert!(state.session.tables().is_empty());
            assert!(state.query.current_result().is_none());
            assert!(state.session.selected_table_key().is_none());
            assert!(state.session.connection_state().is_connecting());
            assert_eq!(state.session.metadata_state(), &MetadataState::Loading);
            test_fixtures::assert_connection_save_fetch_effects(&effects, DatabaseType::SQLite);
        }

        #[test]
        fn save_preserves_connected_cache_before_submit() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::new();
            state.session.activate_connection_with_dsn(
                &current_id,
                "current",
                DatabaseType::PostgreSQL,
                "postgres://localhost/current",
            );
            state.session.mark_connected(Arc::new({
                let mut metadata = DatabaseMetadata::new("current".to_string());
                metadata.table_summaries = vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    None,
                    false,
                )];
                metadata
            }));
            state.ui.set_explorer_selected_raw(4);
            fill_valid_form(&mut state);

            reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());

            let saved = state.connection_caches.get(&current_id).unwrap();
            assert_eq!(saved.explorer_selected, 4);
            assert!(saved.metadata.is_some());
        }

        #[test]
        fn save_completed_removes_stale_connection_cache_for_saved_profile() {
            let mut state = AppState::new("test".to_string());
            let saved_id = ConnectionId::new();
            state.connection_caches.save(
                &saved_id,
                ConnectionCache {
                    metadata: Some(Arc::new({
                        let mut metadata = DatabaseMetadata::new("stale".to_string());
                        metadata.table_summaries = vec![TableSummary::new(
                            "main".to_string(),
                            "old_table".to_string(),
                            None,
                            false,
                        )];
                        metadata
                    })),
                    ..Default::default()
                },
            );

            let action = Action::ConnectionSaveCompleted(ConnectionTarget {
                id: saved_id.clone(),
                dsn: "sqlite:///tmp/new.db".to_string(),
                name: "new.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            reduce(&mut state, &action, Instant::now());

            assert!(state.connection_caches.get(&saved_id).is_none());
        }

        #[test]
        fn sqlite_save_completed_fetches_metadata() {
            let mut state = AppState::new("test".to_string());

            let action = Action::ConnectionSaveCompleted(ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "sqlite:///tmp/app.db".to_string(),
                name: "app.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            let effects = reduce(&mut state, &action, Instant::now()).unwrap();

            assert_eq!(effects.len(), 1);
            test_fixtures::assert_connection_save_fetch_effects(&effects, DatabaseType::SQLite);
            assert_eq!(state.session.dsn(), Some("sqlite:///tmp/app.db"));
            assert_eq!(
                state.session.active_database_type(),
                Some(DatabaseType::SQLite)
            );
            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
        }

        #[test]
        fn save_completed_clears_er_state_from_previous_connection() {
            let mut state = AppState::new("test".to_string());
            state.ui.set_pending_er_picker(true);
            let _ = state.er_preparation.start_waiting_run();
            state
                .er_preparation
                .queue_pending_table("public.users".to_string());

            let action = Action::ConnectionSaveCompleted(ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "sqlite:///tmp/app.db".to_string(),
                name: "app.db".to_string(),
                database_type: DatabaseType::SQLite,
            });
            reduce(&mut state, &action, Instant::now());

            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(state.er_preparation.pending_tables().is_empty());
        }

        #[test]
        fn save_rejects_host_over_limit() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);
            state.connection_setup.host.set_content("a".repeat(256));

            let result =
                reduce_connection_setup(&mut state, &Action::ConnectionSetupSave, Instant::now());

            assert!(result.is_handled());
            assert_eq!(
                state
                    .connection_setup
                    .validation_errors
                    .get(&ConnectionField::Host),
                Some(&"Must be 255 characters or less".to_string())
            );
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

            assert!(state.connection_setup.is_first_run());
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

            assert!(!state.connection_setup.is_first_run());
        }

        #[test]
        fn is_first_run_false_when_already_connected() {
            let mut state = AppState::new("test".to_string());
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/db");

            reduce(
                &mut state,
                &Action::OpenModal(ModalKind::ConnectionSetup),
                Instant::now(),
            );

            assert!(!state.connection_setup.is_first_run());
        }
    }
}
