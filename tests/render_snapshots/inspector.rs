use super::*;

#[test]
fn inspector_indexes_tab_with_data() {
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
    state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Indexes;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_foreign_keys_tab_with_data() {
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
    state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::ForeignKeys;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_triggers_tab_with_data() {
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
    state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Triggers;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_triggers_tab_empty() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    state.ui.set_explorer_selection(Some(0));

    let mut table = fixtures::sample_table_detail();
    table.triggers = vec![];
    let _ = state.session.set_table_detail(table, 0);
    state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Triggers;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_shows_owner_and_comment() {
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
    state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_with_no_metadata() {
    let now = test_instant();
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    state.ui.set_explorer_selection(Some(0));

    let mut table = fixtures::sample_table_detail();
    table.owner = None;
    table.comment = None;
    table.row_count_estimate = None;
    let _ = state.session.set_table_detail(table, 0);
    state.ui.inspector_tab = sabiql::app::inspector_tab::InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
