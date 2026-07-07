use super::*;
use crate::tests::harness::{focus_connection_field, set_connection_input};
use sabiql_app::model::shared::settings::KeymapPreset;

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
        .set_error(ConnectionErrorInfo::with_kind(
            ConnectionErrorKind::HostUnreachable,
            "psql: error: could not translate host name \"db.example.com\" to address",
        ));
    state.connection_error.reset_view();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_error_expanded() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionError);
    state.connection_error.set_error(ConnectionErrorInfo::with_kind(
        ConnectionErrorKind::Timeout,
        "psql: error: connection to server at \"192.168.1.100\", port 5432 failed: timeout expired",
    ));
    state.connection_error.expand_details();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_error_expanded_with_tabs() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionError);
    state.connection_error.set_error(ConnectionErrorInfo::with_kind(
        ConnectionErrorKind::Unknown,
        "psql: error: connection to server at \"localhost\" (127.0.0.1), port 5433 failed: Connection refused\n\tIs the server running on that host and accepting TCP/IP connections?",
    ));
    state.connection_error.expand_details();

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
        .set_error(ConnectionErrorInfo::with_kind(
            ConnectionErrorKind::Unknown,
            &long_details,
        ));
    state.connection_error.expand_details();

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
