use super::*;
use sabiql_app::model::shared::settings::KeymapPreset;

#[test]
fn connection_setup_form() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state
        .connection_setup
        .database
        .set_content("mydb".to_string());
    state
        .connection_setup
        .user
        .set_content("postgres".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_empty_host_focused() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.focused_field = ConnectionField::Host;
    state.connection_setup.host = TextInputState::default();
    state
        .connection_setup
        .database
        .set_content("mydb".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_preview_omits_empty_optional_fields() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.host = TextInputState::default();
    state
        .connection_setup
        .database
        .set_content("mydb".to_string());
    state.connection_setup.user = TextInputState::default();
    state
        .connection_setup
        .password
        .set_content("secret".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_preview_uses_postgres_conninfo_escaping() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state
        .connection_setup
        .host
        .set_content("/var/run/postgresql".to_string());
    state
        .connection_setup
        .database
        .set_content("my'db".to_string());
    state
        .connection_setup
        .user
        .set_content("user'org".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_preview_wraps_across_multiple_rows_for_long_conninfo() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.host.set_content(
        "analytics-primary-read-replica.cluster.internal.example.company.service".to_string(),
    );
    state
        .connection_setup
        .database
        .set_content("warehouse_reporting_environment_for_customer_success_dashboards".to_string());
    state.connection_setup.user.set_content(
        "customer_success_preview_validation_operator_with_extended_scope".to_string(),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_head() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.focused_field = ConnectionField::Host;
    state.connection_setup.host = TextInputState::new("db.example.com", 0);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_middle() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.focused_field = ConnectionField::Host;
    state.connection_setup.host = TextInputState::new("db.example.com", 7);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_tail() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.focused_field = ConnectionField::Host;
    state
        .connection_setup
        .host
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
    state.connection_setup.focused_field = ConnectionField::SslMode;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_with_validation_errors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.host = TextInputState::default();
    state.connection_setup.database = TextInputState::default();
    state.connection_setup.user = TextInputState::default();
    state
        .connection_setup
        .validation_errors
        .insert(ConnectionField::Database, "Required".to_string());

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
    state.connection_error.details_expanded = false;

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
    state.connection_error.details_expanded = true;

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
    state.connection_error.details_expanded = true;

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
    state.connection_error.details_expanded = true;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn footer_shows_success_message() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.messages.last_success = Some("Reconnected!".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
