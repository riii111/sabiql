use super::*;
use crate::tests::harness::{
    focus_connection_field, render_to_string_with_services, set_connection_input,
};
use sabiql_app::model::shared::settings::KeymapPreset;
use sabiql_app::ports::outbound::DsnBuilder;
use sabiql_domain::ConnectionProfile;

struct EmptyPasswordDsnBuilder;

impl DsnBuilder for EmptyPasswordDsnBuilder {
    fn build_dsn(&self, _profile: &ConnectionProfile) -> String {
        "mysql://mysql_user:@localhost:3306/app?ssl-mode=PREFERRED".to_string()
    }
}

fn repeated(ch: char, len: usize) -> String {
    std::iter::repeat_n(ch, len).collect()
}

#[test]
fn connection_setup_form() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state
        .connection_setup
        .input_mut(ConnectionField::Database)
        .unwrap()
        .set_content("mydb".to_string());
    state
        .connection_setup
        .input_mut(ConnectionField::User)
        .unwrap()
        .set_content("postgres".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_sqlite_form() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state
        .connection_setup
        .set_database_type(DatabaseType::SQLite);
    state
        .connection_setup
        .input_mut(ConnectionField::SqlitePath)
        .unwrap()
        .set_content("/tmp/app.db".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_mysql_form() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state
        .connection_setup
        .set_database_type(DatabaseType::MySQL);
    state
        .connection_setup
        .input_mut(ConnectionField::Database)
        .unwrap()
        .set_content("app".to_string());
    state
        .connection_setup
        .input_mut(ConnectionField::User)
        .unwrap()
        .set_content("mysql_user".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_mysql_cleartext_auth_notice() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state
        .connection_setup
        .set_database_type(DatabaseType::MySQL);
    focus_connection_field(&mut state, ConnectionField::CleartextAuth);
    state.connection_setup.toggle_focused_dropdown();
    state.connection_setup.dropdown_next();
    state.connection_setup.confirm_dropdown();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_mysql_preview_does_not_mask_empty_password() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    let mut services = AppServices::stub();
    services.dsn_builder = Arc::new(EmptyPasswordDsnBuilder);

    state.modal.set_mode(InputMode::ConnectionSetup);
    state
        .connection_setup
        .set_database_type(DatabaseType::MySQL);
    state
        .connection_setup
        .input_mut(ConnectionField::Database)
        .unwrap()
        .set_content("app".to_string());
    state
        .connection_setup
        .input_mut(ConnectionField::User)
        .unwrap()
        .set_content("mysql_user".to_string());

    let output = render_to_string_with_services(&mut terminal, &mut state, &services);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_empty_host_focused() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    focus_connection_field(&mut state, ConnectionField::Host);
    set_connection_input(&mut state, ConnectionField::Host, TextInputState::default());
    set_connection_input(
        &mut state,
        ConnectionField::Database,
        TextInputState::new("mydb", 4),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_empty_password_focused() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    focus_connection_field(&mut state, ConnectionField::Password);
    set_connection_input(
        &mut state,
        ConnectionField::Database,
        TextInputState::new("mydb", 4),
    );
    set_connection_input(
        &mut state,
        ConnectionField::Password,
        TextInputState::default(),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_preview_omits_empty_optional_fields() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    set_connection_input(&mut state, ConnectionField::Host, TextInputState::default());
    set_connection_input(
        &mut state,
        ConnectionField::Database,
        TextInputState::new("mydb", 4),
    );
    set_connection_input(&mut state, ConnectionField::User, TextInputState::default());
    set_connection_input(
        &mut state,
        ConnectionField::Password,
        TextInputState::new("secret", 6),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_preview_uses_postgres_conninfo_escaping() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    set_connection_input(
        &mut state,
        ConnectionField::Host,
        TextInputState::new("/var/run/postgresql", 19),
    );
    set_connection_input(
        &mut state,
        ConnectionField::Database,
        TextInputState::new("my'db", 5),
    );
    set_connection_input(
        &mut state,
        ConnectionField::User,
        TextInputState::new("user'org", 8),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_preview_wraps_across_multiple_rows_for_long_conninfo() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    set_connection_input(
        &mut state,
        ConnectionField::Host,
        TextInputState::new(
            "analytics-primary-read-replica.cluster.internal.example.company.service",
            70,
        ),
    );
    set_connection_input(
        &mut state,
        ConnectionField::Database,
        TextInputState::new(
            "warehouse_reporting_environment_for_customer_success_dashboards",
            61,
        ),
    );
    set_connection_input(
        &mut state,
        ConnectionField::User,
        TextInputState::new(
            "customer_success_preview_validation_operator_with_extended_scope",
            63,
        ),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_preview_with_max_length_fields() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    set_connection_input(
        &mut state,
        ConnectionField::Name,
        TextInputState::new(repeated('n', 50), 50),
    );
    set_connection_input(
        &mut state,
        ConnectionField::Host,
        TextInputState::new(repeated('h', 255), 255),
    );
    set_connection_input(
        &mut state,
        ConnectionField::Port,
        TextInputState::new("65535", 5),
    );
    set_connection_input(
        &mut state,
        ConnectionField::Database,
        TextInputState::new(repeated('d', 255), 255),
    );
    set_connection_input(
        &mut state,
        ConnectionField::User,
        TextInputState::new(repeated('u', 255), 255),
    );
    set_connection_input(
        &mut state,
        ConnectionField::Password,
        TextInputState::new(repeated('p', 255), 255),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_head() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    focus_connection_field(&mut state, ConnectionField::Host);
    set_connection_input(
        &mut state,
        ConnectionField::Host,
        TextInputState::new("db.example.com", 0),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_middle() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    focus_connection_field(&mut state, ConnectionField::Host);
    set_connection_input(
        &mut state,
        ConnectionField::Host,
        TextInputState::new("db.example.com", 7),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_tail() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    focus_connection_field(&mut state, ConnectionField::Host);
    state
        .connection_setup
        .input_mut(ConnectionField::Host)
        .unwrap()
        .set_content("db.example.com".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_ssl_mode_ide_hint() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.settings.load_keymap_preset(KeymapPreset::Ide);
    focus_connection_field(&mut state, ConnectionField::SslMode);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_with_validation_errors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    set_connection_input(&mut state, ConnectionField::Host, TextInputState::default());
    set_connection_input(
        &mut state,
        ConnectionField::Database,
        TextInputState::default(),
    );
    set_connection_input(&mut state, ConnectionField::User, TextInputState::default());
    state
        .connection_setup
        .set_validation_error(ConnectionField::Database, "Required");

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_error_collapsed() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionError);
    state
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::HostUnreachable,
                details: "psql: error: could not translate host name \"db.example.com\" to address"
                    .to_string(),
            },
        ));
    state.connection_error.reset_view();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn mysql_active_connection_retryable_error_shows_retry_action() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.session.activate_connection_with_target(
        &sabiql_domain::ConnectionId::from_string("mysql-test"),
        "mysql",
        DatabaseType::MySQL,
        "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
        Some("app"),
    );
    state.modal.set_mode(InputMode::ConnectionError);
    state
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::Timeout(
                "ERROR 2003 (HY000): Can't connect to MySQL server on 'localhost' (110)"
                    .to_string(),
            ),
        ));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

fn render_service_error_without_service_file_hint(save_and_connect: bool) -> String {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    state.session.activate_connection_with_dsn(
        &sabiql_domain::ConnectionId::from_string("service"),
        "service",
        DatabaseType::PostgreSQL,
        "service=mydb",
    );
    state.set_service_file_path(Some(std::path::PathBuf::from("/etc/pg_service.conf")));
    if save_and_connect {
        state.connection_error.set_save_and_connect_error(
            ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                "mysql save failed".to_string(),
            )),
        );
    } else {
        state.connection_error.set_connection_switch_error(
            ConnectionErrorInfo::from_db_operation_error(&DbOperationError::ConnectionFailed(
                "mysql switch failed".to_string(),
            )),
        );
    }
    state.modal.set_mode(InputMode::ConnectionError);

    render_to_string(&mut terminal, &mut state)
}

#[test]
fn connection_error_save_failure_hides_retry_in_modal_and_footer() {
    let output = render_service_error_without_service_file_hint(true);

    assert!(output.contains("Actions:  e  Re-enter"));
    assert!(output.contains("e:Edit"));
    assert!(!output.contains("Retry"));
}

#[test]
fn non_mysql_retryable_error_hides_retry_in_modal_and_footer() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    state.session.activate_connection_with_dsn(
        &sabiql_domain::ConnectionId::from_string("postgres-test"),
        "postgres",
        DatabaseType::PostgreSQL,
        "postgres://localhost:5432/app",
    );
    state.modal.set_mode(InputMode::ConnectionError);
    state
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::Timeout("connection timed out".to_string()),
        ));

    let output = render_to_string(&mut terminal, &mut state);

    assert!(output.contains("Actions:  e  Re-enter"));
    assert!(output.contains("e:Edit"));
    assert!(!output.contains("Retry"));
}

#[test]
fn connection_error_omits_service_file_hint_for_mysql_save() {
    let output = render_service_error_without_service_file_hint(true);

    assert!(!output.contains("pg_service.conf"));
}

#[test]
fn connection_error_omits_service_file_hint_for_mysql_switch() {
    let output = render_service_error_without_service_file_hint(false);

    assert!(!output.contains("pg_service.conf"));
}

#[test]
fn connection_error_expanded() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionError);
    state
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::Timeout(
                "psql: error: connection to server at \"192.168.1.100\", port 5432 failed: timeout expired"
                    .to_string(),
            ),
        ));
    state.connection_error.toggle_details();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_error_expanded_with_tabs() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionError);
    state
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::ConnectionFailed(
                "psql: error: connection to server at \"localhost\" (127.0.0.1), port 5433 failed: Connection refused\n\tIs the server running on that host and accepting TCP/IP connections?"
                    .to_string(),
            ),
        ));
    state.connection_error.toggle_details();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_error_expanded_long_details_capped() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    let long_details = (1..=25)
        .map(|i| format!("ERROR line {i}: something went wrong in module_{i}"))
        .collect::<Vec<_>>()
        .join("\n");

    state.modal.set_mode(InputMode::ConnectionError);
    state
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::ConnectionFailed(long_details),
        ));
    state.connection_error.toggle_details();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn footer_shows_success_message() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .messages
        .set_success_at("Reconnected!".to_string(), std::time::Instant::now());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
