use super::*;

#[test]
fn connection_setup_form() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.database = "mydb".to_string();
    state.connection_setup.user = "postgres".to_string();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_head() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.focused_field = ConnectionField::Host;
    state.connection_setup.host = "db.example.com".to_string();
    state.connection_setup.cursor_position = 0;
    state.connection_setup.viewport_offset = 0;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_middle() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.focused_field = ConnectionField::Host;
    state.connection_setup.host = "db.example.com".to_string();
    state.connection_setup.cursor_position = 7;
    state.connection_setup.viewport_offset = 0;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_cursor_at_tail() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.focused_field = ConnectionField::Host;
    state.connection_setup.host = "db.example.com".to_string();
    state.connection_setup.cursor_position = 14;
    state.connection_setup.viewport_offset = 0;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn connection_setup_with_validation_errors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ConnectionSetup);
    state.connection_setup.host = String::new();
    state.connection_setup.database = String::new();
    state.connection_setup.user = String::new();
    state
        .connection_setup
        .validation_errors
        .insert(ConnectionField::Host, "Required".to_string());
    state
        .connection_setup
        .validation_errors
        .insert(ConnectionField::Database, "Required".to_string());
    state
        .connection_setup
        .validation_errors
        .insert(ConnectionField::User, "Required".to_string());

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
fn footer_shows_success_message() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.messages.last_success = Some("Reconnected!".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
