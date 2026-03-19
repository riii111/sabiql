use super::*;

#[test]
fn table_selection_with_preview() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    state.ui.set_explorer_selection(Some(0));
    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);
    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result(now)));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn focus_on_result_pane() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    state.ui.set_explorer_selection(Some(0));
    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result(now)));
    state.ui.focused_pane = FocusedPane::Result;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn focus_mode_fullscreen_result() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    state.ui.set_explorer_selection(Some(0));
    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result(now)));
    state.ui.focus_mode = true;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn error_message_in_footer() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    state.ui.set_explorer_selection(Some(0));
    state.messages.last_error = Some("Connection failed: timeout".to_string());
    state.messages.expires_at = Some(now + Duration::from_secs(10));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn empty_query_result() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    state.ui.set_explorer_selection(Some(0));
    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);
    state
        .query
        .set_current_result(Arc::new(fixtures::empty_query_result(now)));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
