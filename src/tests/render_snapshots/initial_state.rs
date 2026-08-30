use super::*;
use sabiql_domain::ConnectionId;

#[test]
fn initial_state_no_metadata() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn explorer_shows_not_connected_when_no_active_connection() {
    let mut state = create_test_state();
    state.session.clear_connection();
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn header_shows_effective_user_at_normal_width() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "test",
        DatabaseType::PostgreSQL,
        "postgresql://localhost/test",
    );
    state
        .session
        .mark_effective_user_loaded(Some("app_user".to_string()));
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn mysql_header_shows_effective_user_without_dsn_password() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "test",
        DatabaseType::MySQL,
        "mysql://app:header-secret@localhost/app",
    );
    state
        .session
        .mark_effective_user_loaded(Some("app@%".to_string()));
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    assert!(output.contains("user: app@%"));
    assert!(!output.contains("header-secret"));
}

#[test]
fn header_truncates_connection_name_at_narrow_width() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "very-long-connection-name-that-must-yield-space-to-the-user",
        DatabaseType::PostgreSQL,
        "postgresql://localhost/test",
    );
    state
        .session
        .mark_effective_user_loaded(Some("app_user".to_string()));
    let mut terminal = create_test_terminal_sized(60, 20);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn header_shows_read_only_badge_at_narrow_width() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "test",
        DatabaseType::PostgreSQL,
        "postgresql://localhost/test",
    );
    state.session.enable_read_only();
    state
        .session
        .mark_effective_user_loaded(Some("app_user".to_string()));
    let mut terminal = create_test_terminal_sized(80, 20);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
