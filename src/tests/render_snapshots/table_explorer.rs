use super::*;
use app::model::shared::ui_state::FocusMode;
use harness::{explorer_selected_state, table_detail_loaded_state, with_current_result};

#[test]
fn table_selection_with_preview() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn focus_on_result_pane() {
    let (mut state, now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result(now)));
    state.ui.set_focused_pane(FocusedPane::Result);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn focus_mode_fullscreen_result() {
    let (mut state, now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result(now)));
    state
        .ui
        .set_focus_mode(FocusMode::focused(state.ui.focused_pane()));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn error_message_in_footer() {
    let (mut state, now) = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state
        .messages
        .set_error_at("Connection failed: timeout".to_string(), now);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn empty_query_result() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state
        .query
        .set_current_result(Arc::new(fixtures::empty_query_result(now)));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
