use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::DatabaseType;
use crate::domain::connection::ConnectionProfileError;
use crate::model::app_state::AppState;
use crate::model::connection::error::ConnectionErrorInfo;
use crate::model::connection::setup::{
    CONNECTION_INPUT_VISIBLE_WIDTH, ConnectionField, ConnectionSetupState,
};
use crate::model::connection::state::ConnectionState;
use crate::model::shared::confirm_dialog::ConfirmIntent;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::text_input::TextInputEditing;
use crate::update::action::{
    Action, ConnectionSaveError, ConnectionTarget, InputTarget, ModalKind,
};
use crate::update::connection::helpers::{
    cancel_connection_task_effects, connection_save_fetch_effects,
    mysql_connection_completion_effects, reset_for_new_connection, save_current_connection_cache,
};
use crate::update::connection::lifecycle::try_connect;
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::{validate_all, validate_field};
use crate::update::query_context::termination_effects;

pub fn reduce_connection_setup(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        Action::OpenModal(ModalKind::ConnectionSetup) => {
            state.session.cancel_connection_save_and_disconnect();
            let cancel_effects = cancel_connection_task_effects(state);
            state.connection_setup.reset();
            if !state.connections().is_empty() || state.session.dsn().is_some() {
                state.connection_setup.set_first_run(false);
            }
            state.modal.set_mode(InputMode::ConnectionSetup);
            DispatchResult::handled_with(cancel_effects)
        }
        Action::ConnectionEditLoaded(profile) => {
            state.session.cancel_connection_save_and_disconnect();
            let cancel_effects = cancel_connection_task_effects(state);
            state.connection_setup = ConnectionSetupState::from(&**profile);
            state.modal.set_mode(InputMode::ConnectionSetup);
            DispatchResult::handled_with(cancel_effects)
        }
        Action::ConnectionEditLoadFailed(e) => {
            state.messages.set_error(e.to_string());
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::ConnectionSetup) => {
            state.session.cancel_connection_save_and_disconnect();
            let cancel_effects = cancel_connection_task_effects(state);
            state.modal.set_mode(InputMode::Normal);
            DispatchResult::handled_with(cancel_effects)
        }

        // ===== Clipboard Paste =====
        Action::Paste(text) if state.modal.active_mode() == InputMode::ConnectionSetup => {
            insert_form_text(&mut state.connection_setup, text);
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
                ConnectionField::DatabaseType
                | ConnectionField::Transport
                | ConnectionField::SslMode
                | ConnectionField::CleartextAuth => {}
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
        Action::TextDelete {
            target: InputTarget::ConnectionSetup,
        } => {
            let setup = &mut state.connection_setup;
            if let Some(input) = setup.focused_input_mut() {
                input.delete();
                input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
            }
            DispatchResult::handled()
        }
        Action::TextKill {
            target: InputTarget::ConnectionSetup,
            direction,
        } => {
            let is_password = state.connection_setup.focused_field == ConnectionField::Password;
            let killed = state
                .connection_setup
                .focused_input_mut()
                .map(|input| {
                    let killed = input.kill(*direction);
                    input.update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
                    killed
                })
                .unwrap_or_default();
            if !is_password {
                state.record_kill(killed);
            }
            DispatchResult::handled()
        }
        Action::TextYank {
            target: InputTarget::ConnectionSetup,
        } => {
            if let Some(killed) = state.kill_buffer().map(str::to_owned) {
                insert_form_text(&mut state.connection_setup, &killed);
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
            if state.session.connection_state().is_connecting() {
                return DispatchResult::handled();
            }
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
            if state.session.connection_state() == ConnectionState::Connected {
                save_current_connection_cache(state);
            }
            let run_id = state.session.begin_connection_save();
            let run_guard = state.session.connection_save_guard();
            state.query.reset_for_context_change();
            state.session.clear_mysql_connection_probe();
            state.session.invalidate_connection_generation();
            state.session.mark_connecting();
            DispatchResult::handled_with(termination_effects(
                &state.query,
                vec![Effect::SaveAndConnect {
                    id: state.connection_setup.editing_id().cloned(),
                    name: state
                        .connection_setup
                        .input(ConnectionField::Name)
                        .expect("name is a text input")
                        .content()
                        .trim()
                        .to_string(),
                    config,
                    run_id,
                    run_guard,
                }],
            ))
        }
        Action::ConnectionSetupCancel => {
            state.session.cancel_connection_save_and_disconnect();
            let cancel_effects = cancel_connection_task_effects(state);
            if state.connection_setup.is_first_run() {
                state.confirm_dialog.open(
                    "Confirm",
                    "No connection configured.\nAre you sure you want to quit?",
                    ConfirmIntent::QuitNoConnection,
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
                DispatchResult::handled_with(cancel_effects)
            } else {
                state.modal.set_mode(InputMode::Normal);
                let mut effects = cancel_effects;
                effects.extend(try_connect(state));
                DispatchResult::handled_with(effects)
            }
        }
        Action::ConnectionSaveCompleted {
            target,
            run_id,
            mysql_lower_case_table_names,
            metadata,
        } => {
            if !state.session.is_current_connection_save(*run_id) {
                return DispatchResult::handled();
            }
            state.session.cancel_connection_save();
            let ConnectionTarget {
                id,
                dsn,
                database_type,
                ..
            } = target;
            state.connection_setup.set_first_run(false);
            state.modal.set_mode(InputMode::Normal);
            state.connection_caches.remove(id);

            reset_for_new_connection(state, target);
            if let Some(lower_case_table_names) = mysql_lower_case_table_names {
                state
                    .session
                    .set_mysql_lower_case_table_names(*lower_case_table_names);
            }
            if *database_type == DatabaseType::MySQL {
                return DispatchResult::handled_with(mysql_connection_completion_effects(
                    state, dsn,
                ));
            }
            let run_id = state.session.begin_connecting(dsn);
            DispatchResult::handled_with(connection_save_fetch_effects(
                state,
                dsn,
                run_id,
                metadata.clone(),
            ))
        }
        Action::ConnectionSaveFailed {
            error: e,
            database_type,
            run_id,
        } => {
            if !state.session.is_current_connection_save(*run_id) {
                return DispatchResult::handled();
            }
            state.session.cancel_connection_save();
            if let ConnectionSaveError::Validation(ConnectionProfileError::SqlitePath(error)) = e {
                state
                    .connection_setup
                    .record_sqlite_path_error(error.clone());
            }
            if !state.session.connection_state().is_connected() {
                state.session.mark_disconnected();
            }
            let mysql_error = match e {
                ConnectionSaveError::Probe { error, dsn }
                    if *database_type == DatabaseType::MySQL =>
                {
                    Some(ConnectionErrorInfo::from_db_operation_error(error))
                }
                _ => None,
            };
            if let Some(error_info) = mysql_error {
                state
                    .connection_error
                    .set_save_and_connect_error(error_info);
                state.modal.replace_mode(InputMode::ConnectionError);
                return DispatchResult::handled();
            }
            let message = match e {
                ConnectionSaveError::Metadata(error) => error.user_message(),
                _ => e.to_string(),
            };
            state.messages.set_error(message);
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

fn insert_form_text(setup: &mut ConnectionSetupState, text: &str) {
    let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    match setup.focused_field() {
        ConnectionField::Port => {
            let remaining =
                remaining_input_capacity(ConnectionField::Port, setup.port_mut().char_count());
            let digits: String = clean
                .chars()
                .filter(char::is_ascii_digit)
                .take(remaining)
                .collect();
            if !digits.is_empty() {
                setup.port_mut().insert_str(&digits);
                setup
                    .port_mut()
                    .update_viewport(CONNECTION_INPUT_VISIBLE_WIDTH);
            }
        }
        ConnectionField::DatabaseType
        | ConnectionField::Transport
        | ConnectionField::SslMode
        | ConnectionField::CleartextAuth => {}
        field => {
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
    use crate::services::AppServices;
    use crate::update::action::TextKillDirection;
    use crate::update::connection::error::reduce_connection_error;
    use crate::update::connection::lifecycle::reduce_connection_lifecycle;
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

        #[test]
        fn yank_applies_port_validation_and_limit() {
            let mut state = setup_state_with_field(ConnectionField::Port);
            state.record_kill("12x345678".to_string());

            reduce_connection_setup(
                &mut state,
                &Action::TextYank {
                    target: InputTarget::ConnectionSetup,
                },
                Instant::now(),
            );

            assert_eq!(state.connection_setup.port.content(), "12345");
        }

        #[test]
        fn killing_password_does_not_update_shared_kill_buffer() {
            let mut state = setup_state_with_field(ConnectionField::Password);
            state.record_kill("safe-to-yank".to_string());
            state
                .connection_setup
                .password
                .set_content("secret".to_string());

            reduce_connection_setup(
                &mut state,
                &Action::TextKill {
                    target: InputTarget::ConnectionSetup,
                    direction: TextKillDirection::ToLineStart,
                },
                Instant::now(),
            );

            assert_eq!(state.connection_setup.password.content(), "");
            assert_eq!(state.kill_buffer(), Some("safe-to-yank"));
        }

        #[test]
        fn delete_removes_the_character_at_the_connection_field_cursor() {
            let mut state = setup_state_with_field(ConnectionField::Host);
            state.connection_setup.host.set_content("abc".to_string());
            state.connection_setup.host.set_cursor(1);

            reduce_connection_setup(
                &mut state,
                &Action::TextDelete {
                    target: InputTarget::ConnectionSetup,
                },
                Instant::now(),
            );

            assert_eq!(state.connection_setup.host.content(), "ac");
        }
    }

    mod connection_save {
        use std::sync::Arc;

        use super::*;
        use crate::domain::{
            DatabaseMetadata, MetadataState, QueryResult, QuerySource, TableSummary,
        };
        use crate::model::connection::cache::ConnectionCache;
        use crate::model::connection::error::ConnectionErrorKind;
        use crate::ports::outbound::{ConnectionFailureKind, DbOperationError};

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
        fn cancelled_save_can_be_submitted_again() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);

            reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());
            assert!(state.session.connection_state().is_connecting());

            reduce(&mut state, &Action::ConnectionSetupCancel, Instant::now());
            assert!(state.session.connection_state().is_not_connected());

            state.modal.set_mode(InputMode::ConnectionSetup);
            let effects = reduce(&mut state, &Action::ConnectionSetupSave, Instant::now())
                .expect("second save handled");
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::SaveAndConnect { .. }))
            );
        }

        #[test]
        fn cancelled_save_can_retry_previous_connection() {
            let mut state = AppState::new("test".to_string());
            let previous_id = ConnectionId::from_string("previous");
            state.session.activate_connection_with_dsn(
                &previous_id,
                "previous",
                DatabaseType::PostgreSQL,
                "postgres://localhost/previous",
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            state.modal.set_mode(InputMode::ConnectionSetup);
            state.connection_setup.set_first_run(false);
            fill_valid_form(&mut state);

            reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());
            let effects = reduce(&mut state, &Action::ConnectionSetupCancel, Instant::now())
                .expect("cancel should retry previous connection");

            assert!(state.session.connection_state().is_connecting());
            assert_eq!(state.session.active_connection_id(), Some(&previous_id));
            assert_eq!(state.session.dsn(), Some("postgres://localhost/previous"));
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::FetchMetadata { .. }))
            );
            assert!(state.session.connection_state().is_connecting());
        }

        #[test]
        fn mysql_probe_failure_preserves_adapter_tls_classification() {
            let mut state = AppState::new("test".to_string());
            state
                .connection_setup
                .set_database_type(DatabaseType::MySQL);

            let error = DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::TlsHostnameVerification,
                details: "certificate verification failed".to_string(),
            };
            let dsn = "mysql://user:password@localhost:3306/app?ssl-mode=PREFERRED".to_string();
            let run_id = state.session.begin_connection_save();

            reduce(
                &mut state,
                &Action::ConnectionSaveFailed {
                    error: ConnectionSaveError::Probe { error, dsn },
                    database_type: DatabaseType::MySQL,
                    run_id,
                },
                Instant::now(),
            );

            assert_eq!(state.modal.active_mode(), InputMode::ConnectionError);
            assert_eq!(
                state.connection_error.error_info().unwrap().kind,
                ConnectionErrorKind::MySqlConnectionFailure(
                    ConnectionFailureKind::TlsHostnameVerification
                )
            );
        }

        #[test]
        fn mysql_probe_failure_uses_typed_errors_for_connection_guidance() {
            for (error, expected) in [
                (
                    DbOperationError::PermissionDenied(
                        "ERROR 1044 (42000): Access denied for user 'user' to database 'mysql'"
                            .to_string(),
                    ),
                    ConnectionErrorKind::PermissionDenied,
                ),
                (
                    DbOperationError::ConnectionFailedWithKind {
                        kind: ConnectionFailureKind::Auth,
                        details: "ERROR 1045 (28000): Access denied for user 'user'".to_string(),
                    },
                    ConnectionErrorKind::AuthFailed,
                ),
                (
                    DbOperationError::ConnectionFailedWithKind {
                        kind: ConnectionFailureKind::DatabaseNotFound,
                        details: "ERROR 1049 (42000): Unknown database 'missing'".to_string(),
                    },
                    ConnectionErrorKind::DatabaseNotFound,
                ),
            ] {
                let mut state = AppState::new("test".to_string());
                state
                    .connection_setup
                    .set_database_type(DatabaseType::MySQL);
                let run_id = state.session.begin_connection_save();

                reduce(
                    &mut state,
                    &Action::ConnectionSaveFailed {
                        error: ConnectionSaveError::Probe {
                            error,
                            dsn: "mysql://user:password@localhost:3306/app?ssl-mode=PREFERRED"
                                .to_string(),
                        },
                        database_type: DatabaseType::MySQL,
                        run_id,
                    },
                    Instant::now(),
                );

                let error_info = state.connection_error.error_info().unwrap();
                assert_eq!(error_info.kind, expected);
                if expected == ConnectionErrorKind::PermissionDenied {
                    assert!(!error_info.kind.hint().contains("password"));
                }
            }
        }

        #[test]
        fn save_invalidates_in_flight_mysql_probe() {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::from_string("postgres-a");
            state.session.activate_connection_with_dsn(
                &current_id,
                "postgres-a",
                DatabaseType::PostgreSQL,
                "postgres://localhost/a",
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let target = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };
            let probe_run_id = state.session.begin_mysql_connection_probe(
                &target.id,
                &target.name,
                &target.dsn,
                target.database.as_deref(),
            );

            fill_valid_form(&mut state);
            reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());
            reduce_connection_lifecycle(
                &mut state,
                &Action::MySqlConnectionProbeCompleted {
                    target,
                    run_id: probe_run_id,
                    lower_case_table_names: 0,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.session.active_connection_id(), Some(&current_id));
            assert!(state.session.pending_mysql_connection_probe().is_none());
        }

        #[test]
        fn cancelled_save_completion_is_ignored() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);
            let effects = reduce(&mut state, &Action::ConnectionSetupSave, Instant::now())
                .expect("save handled");
            let run_id = effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::SaveAndConnect { run_id, .. } => Some(*run_id),
                    _ => None,
                })
                .expect("save run id");

            reduce(&mut state, &Action::ConnectionSetupCancel, Instant::now());
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);

            reduce(
                &mut state,
                &Action::ConnectionSaveCompleted {
                    target: ConnectionTarget {
                        id: ConnectionId::new(),
                        dsn: "postgres://localhost/stale".to_string(),
                        name: "stale".to_string(),
                        database_type: DatabaseType::PostgreSQL,
                        database: None,
                    },
                    run_id,
                    mysql_lower_case_table_names: None,
                    metadata: None,
                },
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(state.session.active_connection_id().is_none());
        }

        #[test]
        fn starting_switch_invalidates_previous_save_completion() {
            let mut state = AppState::new("test".to_string());
            let save_run_id = state.session.begin_connection_save();
            let target = ConnectionTarget {
                id: ConnectionId::from_string("mysql-b"),
                dsn: "mysql://user@localhost:3306/b?ssl-mode=PREFERRED".to_string(),
                name: "mysql-b".to_string(),
                database_type: DatabaseType::MySQL,
                database: Some("b".to_string()),
            };

            reduce_connection_lifecycle(
                &mut state,
                &Action::SwitchConnection(target.clone()),
                Instant::now(),
                &AppServices::stub(),
            );
            reduce(
                &mut state,
                &Action::ConnectionSaveCompleted {
                    target: ConnectionTarget {
                        id: ConnectionId::new(),
                        dsn: "postgres://localhost/stale".to_string(),
                        name: "stale".to_string(),
                        database_type: DatabaseType::PostgreSQL,
                        database: None,
                    },
                    run_id: save_run_id,
                    mysql_lower_case_table_names: None,
                    metadata: None,
                },
                Instant::now(),
            );

            assert_eq!(
                state
                    .session
                    .pending_mysql_connection_probe()
                    .map(|pending| &pending.id),
                Some(&target.id)
            );
            assert!(state.session.active_connection_id().is_none());
        }

        #[test]
        fn mysql_save_failure_keeps_retry_from_using_previous_service() {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::from_string("service:mydb"),
                "mydb",
                DatabaseType::PostgreSQL,
                "service=mydb",
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let run_id = state.session.begin_connection_save();

            reduce(
                &mut state,
                &Action::ConnectionSaveFailed {
                    error: ConnectionSaveError::Probe {
                        error: DbOperationError::ConnectionFailed("connection refused".to_string()),
                        dsn: "mysql://user@localhost:3306/app?ssl-mode=PREFERRED".to_string(),
                    },
                    database_type: DatabaseType::MySQL,
                    run_id,
                },
                Instant::now(),
            );

            assert!(state.connection_error.is_save_and_connect_failure());
            assert!(state.connection_error.has_destination());
            let effects =
                reduce_connection_error(&mut state, &Action::RetryConnection, Instant::now())
                    .into_effects()
                    .expect("retry action handled");
            assert!(effects.is_empty());
            assert_eq!(state.input_mode(), InputMode::ConnectionError);

            reduce_connection_error(&mut state, &Action::ReenterConnectionSetup, Instant::now());
            assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        }

        #[test]
        fn postgres_metadata_failure_sets_masked_actionable_message() {
            for error in [
                DbOperationError::ConnectionFailedWithKind {
                    kind: ConnectionFailureKind::Auth,
                    details: "password=secret auth failed".to_string(),
                },
                DbOperationError::ConnectionFailedWithKind {
                    kind: ConnectionFailureKind::ConnectionRefused,
                    details: "password=secret connection refused".to_string(),
                },
                DbOperationError::PermissionDenied("password=secret permission denied".to_string()),
                DbOperationError::MetadataParseFailed(
                    "password=secret malformed output".to_string(),
                ),
            ] {
                let mut state = AppState::new("test".to_string());
                let run_id = state.session.begin_connection_save();
                let expected = error.user_message();

                reduce(
                    &mut state,
                    &Action::ConnectionSaveFailed {
                        error: ConnectionSaveError::Metadata(error),
                        database_type: DatabaseType::PostgreSQL,
                        run_id,
                    },
                    Instant::now(),
                );

                assert_eq!(state.messages.last_error(), Some(expected.as_str()));
                assert!(!state.messages.last_error().unwrap().contains("secret"));
            }
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
                [Effect::CancelTrackedTasks, Effect::SaveAndConnect { .. }]
            ));
        }

        #[test]
        fn repeated_save_while_connecting_does_not_emit_duplicate_effect() {
            let mut state = AppState::new("test".to_string());
            fill_valid_form(&mut state);

            let first_effects = reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());
            assert!(first_effects.is_some_and(|effects| matches!(
                effects.as_slice(),
                [Effect::CancelTrackedTasks, Effect::SaveAndConnect { .. }]
            )));

            let effects = reduce(&mut state, &Action::ConnectionSetupSave, Instant::now());

            assert!(effects.is_some_and(|effects| effects.is_empty()));
            assert!(state.session.connection_state().is_connecting());
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
                [Effect::CancelTrackedTasks, Effect::SaveAndConnect {
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
                [Effect::CancelTrackedTasks, Effect::SaveAndConnect {
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
            let run_id = state.session.begin_connection_save();

            let action = Action::ConnectionSaveCompleted {
                target: ConnectionTarget {
                    id: ConnectionId::new(),
                    dsn: "postgres://localhost/new_db".to_string(),
                    name: "new_db".to_string(),
                    database_type: DatabaseType::PostgreSQL,
                    database: None,
                },
                run_id,
                mysql_lower_case_table_names: None,
                metadata: None,
            };
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

            let run_id = state.session.begin_connection_save();
            let action = Action::ConnectionSaveCompleted {
                target: ConnectionTarget {
                    id: ConnectionId::new(),
                    dsn: "sqlite:///tmp/new.db".to_string(),
                    name: "new.db".to_string(),
                    database_type: DatabaseType::SQLite,
                    database: None,
                },
                run_id,
                mysql_lower_case_table_names: None,
                metadata: None,
            };
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

            let run_id = state.session.begin_connection_save();
            let action = Action::ConnectionSaveCompleted {
                target: ConnectionTarget {
                    id: saved_id.clone(),
                    dsn: "sqlite:///tmp/new.db".to_string(),
                    name: "new.db".to_string(),
                    database_type: DatabaseType::SQLite,
                    database: None,
                },
                run_id,
                mysql_lower_case_table_names: None,
                metadata: None,
            };
            reduce(&mut state, &action, Instant::now());

            assert!(state.connection_caches.get(&saved_id).is_none());
        }

        #[test]
        fn sqlite_save_completed_fetches_metadata() {
            let mut state = AppState::new("test".to_string());

            let run_id = state.session.begin_connection_save();
            let action = Action::ConnectionSaveCompleted {
                target: ConnectionTarget {
                    id: ConnectionId::new(),
                    dsn: "sqlite:///tmp/app.db".to_string(),
                    name: "app.db".to_string(),
                    database_type: DatabaseType::SQLite,
                    database: None,
                },
                run_id,
                mysql_lower_case_table_names: None,
                metadata: None,
            };
            let effects = reduce(&mut state, &action, Instant::now()).unwrap();

            assert_eq!(effects.len(), 3);
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
                .table_prefetch
                .queue_table_prefetch("public.users".to_string());
            let run_id = state.session.begin_connection_save();

            let action = Action::ConnectionSaveCompleted {
                target: ConnectionTarget {
                    id: ConnectionId::new(),
                    dsn: "sqlite:///tmp/app.db".to_string(),
                    name: "app.db".to_string(),
                    database_type: DatabaseType::SQLite,
                    database: None,
                },
                run_id,
                mysql_lower_case_table_names: None,
                metadata: None,
            };
            reduce(&mut state, &action, Instant::now());

            assert!(!state.ui.pending_er_picker());
            assert_eq!(state.er_preparation.status(), ErStatus::Idle);
            assert!(!state.table_prefetch.has_pending_prefetch());
        }

        #[test]
        fn mysql_save_completed_fetches_metadata_for_selected_database() {
            let mut state = AppState::new("test".to_string());
            let run_id = state.session.begin_connection_save();
            let action = Action::ConnectionSaveCompleted {
                target: ConnectionTarget {
                    id: ConnectionId::new(),
                    dsn: "mysql://user@localhost:3306/app".to_string(),
                    name: "mysql".to_string(),
                    database_type: DatabaseType::MySQL,
                    database: Some("app".to_string()),
                },
                run_id,
                mysql_lower_case_table_names: None,
                metadata: None,
            };

            let effects = reduce(&mut state, &action, Instant::now()).unwrap();

            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::FetchMetadata { dsn, .. } if dsn == "mysql://user@localhost:3306/app"
            )));
            assert_eq!(
                state.session.connection_state(),
                ConnectionState::Connecting
            );
            assert_eq!(state.session.metadata_state(), &MetadataState::Loading);
            assert_eq!(state.input_mode(), InputMode::Normal);
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

    mod connection_task_cancellation {
        use super::*;

        fn state_with_pending_mysql_probe() -> AppState {
            let mut state = AppState::new("test".to_string());
            let id = ConnectionId::from_string("mysql-pending");
            let _ = state.session.begin_mysql_connection_probe(
                &id,
                "mysql",
                "mysql://localhost/app",
                Some("app"),
            );
            state
        }

        fn state_with_interrupted_table_detail() -> AppState {
            let mut state = AppState::new("test".to_string());
            let current_id = ConnectionId::from_string("mysql-current");
            let target_id = ConnectionId::from_string("mysql-target");
            state.session.activate_connection_with_target(
                &current_id,
                "mysql-current",
                DatabaseType::MySQL,
                "mysql://localhost/current",
                Some("current"),
            );
            state
                .session
                .set_connection_state(ConnectionState::Connected);
            let _ = state
                .session
                .select_table("public", "users", &mut state.query);
            let _ = state.session.begin_table_detail_run();
            let _ = state.session.begin_mysql_connection_probe(
                &target_id,
                "mysql-target",
                "mysql://localhost/target",
                Some("target"),
            );
            state
        }

        fn assert_table_detail_retry(effects: &[Effect]) {
            assert!(matches!(
                effects,
                [
                    Effect::CancelConnectionTask,
                    Effect::FetchTableDetail {
                        dsn,
                        schema,
                        table,
                        ..
                    }
                ] if dsn == "mysql://localhost/current"
                    && schema == "public"
                    && table == "users"
            ));
        }

        #[test]
        fn opening_setup_clears_pending_probe() {
            let mut state = state_with_pending_mysql_probe();

            let effects = reduce(
                &mut state,
                &Action::OpenModal(ModalKind::ConnectionSetup),
                Instant::now(),
            )
            .unwrap();

            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert!(matches!(effects.as_slice(), [Effect::CancelConnectionTask]));
        }

        #[test]
        fn opening_setup_retries_interrupted_table_detail() {
            let mut state = state_with_interrupted_table_detail();

            let effects = reduce(
                &mut state,
                &Action::OpenModal(ModalKind::ConnectionSetup),
                Instant::now(),
            )
            .unwrap();

            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert_table_detail_retry(&effects);
        }

        #[test]
        fn loading_edit_clears_pending_probe() {
            let mut state = state_with_pending_mysql_probe();

            let effects = reduce(
                &mut state,
                &Action::ConnectionEditLoaded(Box::new(create_profile("edited"))),
                Instant::now(),
            )
            .unwrap();

            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert!(matches!(effects.as_slice(), [Effect::CancelConnectionTask]));
        }

        #[test]
        fn loading_edit_retries_interrupted_table_detail() {
            let mut state = state_with_interrupted_table_detail();

            let effects = reduce(
                &mut state,
                &Action::ConnectionEditLoaded(Box::new(create_profile("edited"))),
                Instant::now(),
            )
            .unwrap();

            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert_table_detail_retry(&effects);
        }

        #[test]
        fn closing_setup_clears_pending_probe() {
            let mut state = state_with_pending_mysql_probe();
            state.modal.set_mode(InputMode::ConnectionSetup);

            let effects = reduce(
                &mut state,
                &Action::CloseModal(ModalKind::ConnectionSetup),
                Instant::now(),
            )
            .unwrap();

            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert!(matches!(effects.as_slice(), [Effect::CancelConnectionTask]));
        }

        #[test]
        fn closing_setup_retries_interrupted_table_detail() {
            let mut state = state_with_interrupted_table_detail();
            state.modal.set_mode(InputMode::ConnectionSetup);

            let effects = reduce(
                &mut state,
                &Action::CloseModal(ModalKind::ConnectionSetup),
                Instant::now(),
            )
            .unwrap();

            assert!(state.session.pending_mysql_connection_probe().is_none());
            assert_table_detail_retry(&effects);
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
