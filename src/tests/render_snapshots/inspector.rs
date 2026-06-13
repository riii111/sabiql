use super::*;
use harness::table_detail_loaded_state;
use sabiql_app::model::shared::inspector_tab::InspectorTab;

/// Keep this focused snapshot free of right-padding whitespace so diff-check stays stable.
fn trim_line_endings(output: String) -> String {
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn inspector_columns_narrow_pane_keeps_horizontal_scroll() {
    let mut state = harness::explorer_selected_state();
    // Split-pane terminal: the comment column alone exceeds the pane width
    let mut terminal = create_test_terminal_sized(110, 40);

    let mut table = fixtures::sample_table_detail();
    table.columns[0].comment = Some(
        "Primary key, generated from the tenant sequence and never reused after deletion"
            .to_string(),
    );
    let _ = state.session.set_table_detail(table, 0);
    state.ui.set_inspector_tab(InspectorTab::Columns);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_columns_narrow_pane_caps_wide_comment() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal_sized(110, 40);

    let mut table = fixtures::sample_table_detail();
    table.columns[0].comment = Some(
        "Primary key, generated from the tenant sequence and never reused after deletion"
            .to_string(),
    );
    let _ = state.session.set_table_detail(table, 0);
    state.ui.set_inspector_tab(InspectorTab::Columns);
    state.ui.set_focused_pane(FocusedPane::Inspector);
    // Scrolled to the comment column: its width is capped so the preceding
    // columns stay visible instead of the comment monopolizing the pane
    state.ui.set_inspector_horizontal_offset(1);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_columns_narrow_pane_right_edge_truncates_cjk_comment() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal_sized(110, 40);

    // CJK renders two cells per char; the comment must end with an ellipsis
    // instead of being clipped mid-text at the pane border
    let mut table = fixtures::sample_table_detail();
    table.columns[0].comment = Some(
        "ステータス（PENDING:判断待ち、APPROVED:承認済み、REJECTED:却下済み、CANCELED:取消済み）"
            .to_string(),
    );
    let _ = state.session.set_table_detail(table, 0);
    state.ui.set_inspector_tab(InspectorTab::Columns);
    state.ui.set_focused_pane(FocusedPane::Inspector);
    state.ui.set_inspector_horizontal_offset(usize::MAX);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_indexes_tab_with_data() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::Indexes);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_foreign_keys_tab_with_data() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::ForeignKeys);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_triggers_tab_with_data() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::Triggers);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_triggers_tab_empty() {
    let mut state = harness::explorer_selected_state();
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
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.set_inspector_tab(InspectorTab::Info);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_with_no_metadata() {
    let mut state = harness::explorer_selected_state();
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
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    table.owner = Some("postgres".to_string());
    table.comment = Some("SQLite should hide this comment".to_string());
    table.rls = None;
    table.triggers = vec![];
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection_with_dsn(
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
