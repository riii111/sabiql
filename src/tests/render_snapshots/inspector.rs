use super::*;
use harness::table_detail_loaded_state;
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::shared::inspector_tab::InspectorTab;
use sabiql_domain::{
    ColumnGenerationKind, ConnectionId, TableKind, TableKindInfo, TableStorageAttributes,
};

fn sqlite_indexes_state(index: Index) -> AppState {
    let mut state = harness::explorer_selected_state();
    let mut table = fixtures::sample_postgres_table_detail();
    table.indexes = vec![index];
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        &endpoint::sqlite("sqlite:///tmp/app.db"),
    );
    state.ui.inspector_tab = InspectorTab::Indexes;
    state.ui.focused_pane = FocusedPane::Inspector;
    state
}

#[test]
fn inspector_columns_narrow_pane_keeps_horizontal_scroll() {
    let mut state = harness::explorer_selected_state();
    // Split-pane terminal: the comment column alone exceeds the pane width
    let mut terminal = create_test_terminal_sized(110, 40);

    let mut table = fixtures::sample_postgres_table_detail();
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

    let mut table = fixtures::sample_postgres_table_detail();
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
    let mut table = fixtures::sample_postgres_table_detail();
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
fn inspector_columns_marks_read_only_generated_columns() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    table.columns[1].attributes =
        ColumnAttributes::READ_ONLY | ColumnAttributes::GENERATED | ColumnAttributes::NULLABLE;
    let _ = state.session.set_table_detail(table, 0);
    state.ui.inspector_tab = InspectorTab::Columns;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_columns_shows_mysql_column_metadata() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    table.columns[1].character_set_name = Some("utf8mb4".to_string());
    table.columns[1].collation_name = Some("utf8mb4_bin".to_string());
    table.columns[2].attributes =
        ColumnAttributes::READ_ONLY | ColumnAttributes::GENERATED | ColumnAttributes::NULLABLE;
    table.columns[2].generation_expression = Some("(`id` * 2)".to_string());
    table.columns[2].generation_kind = Some(ColumnGenerationKind::Stored);
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("mysql-test"),
        "app",
        &endpoint::mysql("mysql://user@localhost:3306/app?ssl-mode=PREFERRED", None),
    );
    state.ui.inspector_tab = InspectorTab::Columns;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

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
fn inspector_indexes_tab_for_mysql_hides_unsupported_partial_column() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    table.indexes = vec![Index {
        name: "idx_users_email_lower".to_string(),
        columns: vec!["lower(email)".to_string()],
        attributes: IndexAttributes::EXPRESSION | IndexAttributes::HAS_AUXILIARY_COLUMNS,
        index_type: IndexType::BTree,
        definition: Some(
            "CREATE INDEX idx_users_email_lower ON users ((lower(`email`)))".to_string(),
        ),
    }];
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("mysql-test"),
        "app",
        &endpoint::mysql("mysql://user@localhost:3306/app?ssl-mode=PREFERRED", None),
    );
    state.ui.inspector_tab = InspectorTab::Indexes;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_indexes_tab_for_sqlite_hides_unknown_type() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    for index in &mut table.indexes {
        index.index_type = IndexType::Unknown;
    }
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        &endpoint::sqlite("sqlite:///tmp/app.db"),
    );
    state.ui.inspector_tab = InspectorTab::Indexes;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_indexes_tab_shows_sqlite_index_definitions() {
    let cases = [
        (
            "idx_users_email_active",
            "email",
            IndexAttributes::PARTIAL,
            "CREATE INDEX idx_users_email_active ON users(email) WHERE email IS NOT NULL",
            true,
        ),
        (
            "idx_users_name_desc",
            "name",
            IndexAttributes::DESCENDING,
            "CREATE INDEX idx_users_name_desc ON users(name DESC)",
            false,
        ),
        (
            "idx_users_name_nocase",
            "name",
            IndexAttributes::NON_BINARY_COLLATION,
            "CREATE INDEX idx_users_name_nocase ON users(name COLLATE NOCASE)",
            false,
        ),
        (
            "idx_users_email_lower",
            "<expression>",
            IndexAttributes::PARTIAL
                | IndexAttributes::EXPRESSION
                | IndexAttributes::HAS_AUXILIARY_COLUMNS,
            "CREATE INDEX idx_users_email_lower ON users(lower(email)) WHERE email IS NOT NULL",
            true,
        ),
    ];
    let mut table = fixtures::sample_postgres_table_detail();
    table.indexes = cases
        .iter()
        .map(|(name, column, attributes, definition, _)| Index {
            name: (*name).to_string(),
            columns: vec![(*column).to_string()],
            attributes: *attributes,
            index_type: IndexType::Unknown,
            definition: Some((*definition).to_string()),
        })
        .collect();
    let mut state = sqlite_indexes_state(table.indexes[0].clone());
    let mut terminal = create_test_terminal();
    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));
    insta::assert_snapshot!(
        "inspector_indexes_tab_shows_sqlite_partial_index_definition",
        output
    );

    let _ = state.session.set_table_detail(table, 0);
    let mut terminal = create_test_terminal_sized(240, 40);
    let output = render_to_string(&mut terminal, &mut state);

    let header = output
        .lines()
        .find(|line| line.contains("Name") && line.contains("Partial") && line.contains("Detail"))
        .expect("SQLite index headers");
    assert!(!header.contains("Type"), "{header}");
    for (name, column, _, definition, partial) in cases {
        let row = output
            .lines()
            .find(|line| line.contains(definition))
            .unwrap_or_else(|| panic!("missing definition: {definition}"));
        let cells = row
            .split_once("CREATE INDEX")
            .unwrap()
            .0
            .rsplit_once("││")
            .unwrap()
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        if partial {
            assert_eq!(cells, [name, column, "✓"], "{row}");
        } else {
            assert_eq!(cells, [name, column], "{row}");
        }
    }
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
fn inspector_foreign_keys_tab_marks_unresolved_reference() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();
    let mut table = fixtures::sample_postgres_table_detail();
    table.foreign_keys.push(ForeignKey {
        name: "fk_users_missing_org".to_string(),
        from_schema: "public".to_string(),
        from_table: "users".to_string(),
        from_columns: vec!["org_id".to_string()],
        to_schema: "public".to_string(),
        to_table: "missing_orgs".to_string(),
        to_columns: vec!["id".to_string()],
        on_delete: FkAction::NoAction,
        on_update: FkAction::NoAction,
        reference_resolved: false,
    });
    let _ = state.session.set_table_detail(table, 0);

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

    let mut table = fixtures::sample_postgres_table_detail();
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

    let mut table = fixtures::sample_postgres_table_detail();
    table.owner = None;
    table.comment = None;
    table.row_count_estimate = None;
    let _ = state.session.set_table_detail(table, 0);
    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_for_sqlite_hides_postgres_only_fields() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    table.owner = Some("postgres".to_string());
    table.comment = Some("SQLite should hide this comment".to_string());
    table.rls = None;
    table.triggers = vec![];
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        &endpoint::sqlite("sqlite:///tmp/app.db"),
    );
    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_for_mysql_hides_schema_field() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let _ = state
        .session
        .set_table_detail(fixtures::sample_postgres_table_detail(), 0);
    state.session.activate_connection(
        &ConnectionId::from_string("mysql-test"),
        "app",
        &endpoint::mysql("mysql://user@localhost:3306/app?ssl-mode=PREFERRED", None),
    );
    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_for_mysql_shows_storage_attributes() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    table.storage_attributes = TableStorageAttributes {
        engine: Some("InnoDB".to_string()),
        row_format: Some("Compressed".to_string()),
        table_collation: Some("utf8mb4_bin".to_string()),
        create_options: Some("partitioned".to_string()),
    };
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("mysql-test"),
        "app",
        &endpoint::mysql("mysql://user@localhost:3306/app?ssl-mode=PREFERRED", None),
    );
    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

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

    let mut table = fixtures::sample_postgres_table_detail();
    table.source_ddl = Some(
        "CREATE VIRTUAL TABLE users USING fts5(name, email);\n-- source ddl is not rebuilt"
            .to_string(),
    );
    let _ = state.session.set_table_detail(table, 0);
    state.ui.inspector_tab = InspectorTab::Ddl;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&harness::render_to_string_with_services(
        &mut terminal,
        &mut state,
        &services,
    ));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_for_sqlite_shows_table_kind_fields() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    table.schema = "main".to_string();
    table.name = "notes_fts".to_string();
    table.owner = None;
    table.comment = None;
    table.row_count_estimate = Some(42);
    table.kind_info = TableKindInfo {
        kind: TableKind::Virtual,
        is_strict: false,
        without_rowid: false,
        virtual_module: Some("fts5".to_string()),
    };
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        &endpoint::sqlite("sqlite:///tmp/app.db"),
    );
    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn inspector_info_tab_for_sqlite_shows_view_kind() {
    let mut state = harness::explorer_selected_state();
    let mut terminal = create_test_terminal();

    let mut table = fixtures::sample_postgres_table_detail();
    table.schema = "main".to_string();
    table.name = "active_users".to_string();
    table.owner = None;
    table.comment = None;
    table.row_count_estimate = Some(2);
    table.kind_info = table::view_kind_info();
    let _ = state.session.set_table_detail(table, 0);
    state.session.activate_connection(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        &endpoint::sqlite("sqlite:///tmp/app.db"),
    );
    state.ui.inspector_tab = InspectorTab::Info;
    state.ui.focused_pane = FocusedPane::Inspector;

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}

#[test]
fn nerd_font_marks_columns_and_keys_in_the_schema_browser() {
    use sabiql_app::model::shared::settings::IconMode;

    let mut state = harness::explorer_selected_state();
    state.settings.load_icon_mode(IconMode::NerdFont);
    let mut table = fixtures::sample_postgres_table_detail();
    let mut department = table.columns[2].clone();
    department.name = "department_id".into();
    department.data_type = "integer".into();
    department.ordinal_position = 4;
    table.columns.push(department);
    let generation = state
        .session
        .select_table("public", "users", &mut state.query);
    assert!(state.session.set_table_detail(table, generation));
    state.ui.inspector_tab = InspectorTab::Columns;
    harness::with_current_result(&mut state);
    let mut terminal = create_test_terminal_sized(120, 25);

    let output = trim_line_endings(&render_to_string(&mut terminal, &mut state));

    insta::assert_snapshot!(output);
}
