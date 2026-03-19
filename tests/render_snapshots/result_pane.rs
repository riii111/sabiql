use super::*;

#[test]
fn result_pane_row_active_mode() {
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
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(0);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_active_mode() {
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
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(1);
    state.result_interaction.enter_cell(2);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_mode() {
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
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(1);
    state.result_interaction.enter_cell(2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .cell_edit_input_mut()
        .set_content("new@example.com".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_cursor_at_head() {
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
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(1);
    state.result_interaction.enter_cell(2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state.result_interaction.cell_edit_input_mut().set_cursor(0);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_edit_cursor_at_middle() {
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
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(1);
    state.result_interaction.enter_cell(2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state.result_interaction.cell_edit_input_mut().set_cursor(7);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_cell_active_pending_draft() {
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
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(1);
    state.result_interaction.enter_cell(2);
    state.modal.set_mode(InputMode::Normal);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .cell_edit_input_mut()
        .set_content("new@example.com".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn result_pane_staged_delete_row() {
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
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(0);
    state.result_interaction.stage_row(1);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
