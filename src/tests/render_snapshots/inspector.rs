use super::*;
use harness::table_detail_loaded_state;
use sabiql_app::model::shared::inspector_tab::InspectorTab;

fn trim_line_endings(output: &str) -> String {
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
fn inspector_columns_marks_read_only_generated_columns() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    table.columns[1].attributes =
        ColumnAttributes::READ_ONLY | ColumnAttributes::GENERATED | ColumnAttributes::NULLABLE;
    let _ = state.session.set_table_detail(table, 0);
    state.ui.set_inspector_tab(InspectorTab::Columns);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

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
fn inspector_indexes_tab_for_sqlite_hides_unknown_type() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    for index in &mut table.indexes {
        index.index_type = IndexType::Unknown;
    }
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection_with_dsn(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        DatabaseType::SQLite,
        "sqlite:///tmp/app.db",
    );
    state.ui.set_inspector_tab(InspectorTab::Indexes);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_indexes_tab_shows_sqlite_partial_index_definition() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    table.indexes = vec![Index {
        name: "idx_users_email_active".to_string(),
        columns: vec!["email".to_string()],
        attributes: IndexAttributes::PARTIAL,
        index_type: IndexType::Unknown,
        definition: Some(
            "CREATE INDEX idx_users_email_active ON users(email) WHERE email IS NOT NULL"
                .to_string(),
        ),
    }];
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection_with_dsn(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        DatabaseType::SQLite,
        "sqlite:///tmp/app.db",
    );
    state.ui.set_inspector_tab(InspectorTab::Indexes);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_indexes_tab_shows_sqlite_partial_expression_details() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_table_detail();
    table.indexes = vec![Index {
        name: "idx_users_email_lower".to_string(),
        columns: vec!["<expression>".to_string()],
        attributes: IndexAttributes::PARTIAL
            | IndexAttributes::EXPRESSION
            | IndexAttributes::HAS_AUXILIARY_COLUMNS,
        index_type: IndexType::Unknown,
        definition: Some(
            "CREATE INDEX idx_users_email_lower ON users(lower(email)) WHERE email IS NOT NULL"
                .to_string(),
        ),
    }];
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection_with_dsn(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        DatabaseType::SQLite,
        "sqlite:///tmp/app.db",
    );
    state.ui.set_inspector_tab(InspectorTab::Indexes);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

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

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_ddl_tab_uses_source_ddl() {
    struct SourceDdlGenerator;
    impl DdlGenerator for SourceDdlGenerator {
        fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
            table.source_ddl().unwrap_or_default().to_string()
        }
    }

    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();
    let mut services = AppServices::stub();
    services.ddl_generator = Arc::new(SourceDdlGenerator);

    let mut table = fixtures::sample_table_detail();
    table.source_ddl = Some(
        "CREATE VIRTUAL TABLE users USING fts5(name, email);\n-- source ddl is not rebuilt"
            .to_string(),
    );
    let _ = state.session.set_table_detail(table, 0);
    state.ui.set_inspector_tab(InspectorTab::Ddl);
    state.ui.set_focused_pane(FocusedPane::Inspector);

    let output = trim_line_endings(&harness::render_to_string_with_services(
        &mut terminal,
        &mut state,
        &services,
    ));

    insta::assert_snapshot!(output);
}
