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
    state.session.set_active_connection_name_for_test(None);
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
