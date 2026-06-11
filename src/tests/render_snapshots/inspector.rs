use super::*;
use harness::table_detail_loaded_state;
use sabiql_app::model::shared::inspector_tab::InspectorTab;

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
    state.ui.inspector_tab = InspectorTab::Columns;
    state.ui.focused_pane = FocusedPane::Inspector;

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
    state.ui.inspector_tab = InspectorTab::Columns;
    state.ui.focused_pane = FocusedPane::Inspector;
    // Scrolled to the comment column: its width is capped so the preceding
    // columns stay visible instead of the comment monopolizing the pane
    state.ui.inspector_horizontal_offset = 1;

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
    state.ui.inspector_tab = InspectorTab::Columns;
    state.ui.focused_pane = FocusedPane::Inspector;
    state.ui.inspector_horizontal_offset = usize::MAX;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_indexes_tab_with_data() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.inspector_tab = InspectorTab::Indexes;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_foreign_keys_tab_with_data() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.inspector_tab = InspectorTab::ForeignKeys;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_triggers_tab_with_data() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.inspector_tab = InspectorTab::Triggers;
    state.ui.focused_pane = FocusedPane::Inspector;

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
    state.ui.inspector_tab = InspectorTab::Triggers;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_shows_owner_and_comment() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

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
    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
