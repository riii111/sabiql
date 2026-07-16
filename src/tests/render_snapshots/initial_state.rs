use super::*;

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
    state.session.active_connection_name = None;
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn header_shows_effective_user_at_normal_width() {
    let mut state = connected_state();
    state.session.dsn = Some("postgresql://localhost/test".to_string());
    state
        .session
        .mark_effective_user_loaded(Some("app_user".to_string()));
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn header_truncates_connection_name_at_narrow_width() {
    let mut state = connected_state();
    state.session.dsn = Some("postgresql://localhost/test".to_string());
    state.session.active_connection_name =
        Some("very-long-connection-name-that-must-yield-space-to-the-user".to_string());
    state
        .session
        .mark_effective_user_loaded(Some("app_user".to_string()));
    let mut terminal = create_test_terminal_sized(60, 20);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
