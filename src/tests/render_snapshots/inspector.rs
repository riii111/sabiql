use super::*;
use app::model::shared::inspector_tab::InspectorTab;
use harness::table_detail_loaded_state;

#[test]
fn inspector_indexes_tab_with_data() {
    let (mut state, _now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::Indexes);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_foreign_keys_tab_with_data() {
    let (mut state, _now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::ForeignKeys);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_triggers_tab_with_data() {
    let (mut state, _now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::Triggers);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_triggers_tab_empty() {
    let (mut state, _now) = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    table.triggers = vec![];
    let _ = state.session.set_table_detail(table, 0);
    state.ui.set_inspector_tab(InspectorTab::Triggers);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_shows_owner_and_comment() {
    let (mut state, _now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::Info);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_with_no_metadata() {
    let (mut state, _now) = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    table.owner = None;
    table.comment = None;
    table.row_count_estimate = None;
    let _ = state.session.set_table_detail(table, 0);
    state.ui.set_inspector_tab(InspectorTab::Info);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
