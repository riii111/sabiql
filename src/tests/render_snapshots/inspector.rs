use super::*;
use harness::table_detail_loaded_state;
use sabiql_app::model::shared::inspector_tab::InspectorTab;

fn trim_line_endings(output: String) -> String {
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

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

#[test]
fn inspector_info_tab_for_sqlite_hides_postgres_only_fields() {
    let (mut state, _now) = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    table.owner = None;
    table.comment = None;
    table.rls = None;
    table.triggers = vec![];
    let _ = state.session.set_table_detail(table, 0);
    state.session.set_active_connection_with_dsn(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        DatabaseType::SQLite,
        "sqlite:///tmp/app.db",
    );
    state.ui.set_inspector_tab(InspectorTab::Info);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = trim_line_endings(render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}
