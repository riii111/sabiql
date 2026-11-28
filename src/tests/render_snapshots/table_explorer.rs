use super::*;
use harness::{explorer_selected_state, table_detail_loaded_state, with_current_result};
use sabiql_app::model::shared::ui_state::FocusMode;
use sabiql_domain::{
    ConnectionId, DatabaseMetadata, Schema, TableKind, TableKindInfo, TableSummary,
};

#[test]
fn table_selection_with_preview() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn focus_on_result_pane() {
    let mut state = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result()));
    state.ui.set_focused_pane(FocusedPane::Result);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn focus_mode_fullscreen_result() {
    let mut state = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result()));
    state
        .ui
        .set_focus_mode(FocusMode::focused(state.ui.focused_pane()));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn error_message_in_footer() {
    let mut state = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state
        .messages
        .set_error("Connection failed: timeout".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn long_error_message_wraps_into_footer_and_command_line() {
    let mut state = explorer_selected_state();
    let mut terminal = create_test_terminal();

    state.messages.set_error(
        "Connection failed: the database server returned a detailed explanation that should remain visible until the next user operation instead of disappearing from the footer after a short timeout".to_string(),
    );

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn empty_query_result() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    state
        .query
        .set_current_result(Arc::new(fixtures::empty_query_result()));

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sqlite_explorer_shows_table_names_without_schema_or_kind_suffixes() {
    let mut state = create_test_state();
    state.session.activate_connection(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        &endpoint::sqlite("sqlite:///tmp/app.db"),
    );
    let metadata = {
        let mut metadata = DatabaseMetadata::new("app.db".to_string());
        metadata.schemas = vec![Schema::new("main")];
        metadata.table_summaries = vec![
            TableSummary::new("main".to_string(), "users".to_string(), None, false),
            TableSummary::new("main".to_string(), "settings".to_string(), None, false)
                .with_kind_info(TableKindInfo {
                    without_rowid: true,
                    ..TableKindInfo::default()
                }),
            TableSummary::new("main".to_string(), "notes_fts".to_string(), None, false)
                .with_kind_info(TableKindInfo {
                    kind: TableKind::Virtual,
                    virtual_module: Some("fts5".to_string()),
                    ..TableKindInfo::default()
                }),
            TableSummary::new("main".to_string(), "typed_users".to_string(), None, false)
                .with_kind_info(TableKindInfo {
                    is_strict: true,
                    ..TableKindInfo::default()
                }),
        ];
        metadata
    };
    state.session.mark_connected(Arc::new(metadata));
    state.ui.set_explorer_selection(Some(0));

    let mut terminal = create_test_terminal();
    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn sqlite_header_shows_table_name_without_schema() {
    let mut state = create_test_state();
    state.session.activate_connection(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        &endpoint::sqlite("sqlite:///tmp/app.db"),
    );
    let mut metadata = DatabaseMetadata::new("app.db".to_string());
    metadata.schemas = vec![Schema::new("main")];
    metadata.table_summaries = vec![TableSummary::new(
        "main".to_string(),
        "users".to_string(),
        None,
        false,
    )];
    state.session.mark_connected(Arc::new(metadata));
    let _ = state
        .session
        .select_table("main", "users", &mut state.query);

    let mut terminal = create_test_terminal();
    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn mysql_header_and_explorer_show_table_names() {
    let mut state = create_test_state();
    state.session.activate_connection(
        &ConnectionId::from_string("mysql-test"),
        "mysql",
        &endpoint::mysql(
            "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
            Some("app"),
        ),
    );
    let mut metadata = DatabaseMetadata::new("app".to_string());
    metadata.table_summaries = vec![
        TableSummary::new("app".to_string(), "users".to_string(), None, false),
        TableSummary::new("app".to_string(), "audit_log".to_string(), None, false),
    ];
    state.session.mark_connected(Arc::new(metadata));
    let generation = state.session.select_table("app", "users", &mut state.query);
    assert!(
        state
            .session
            .set_table_detail(table::minimal("app", "users"), generation)
    );
    state.ui.set_explorer_selection(Some(0));

    let mut terminal = create_test_terminal();
    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn view_icon_stays_visible_when_scrolling_a_wide_name_to_its_end() {
    use sabiql_app::model::shared::settings::IconMode;

    let mut state = create_test_state();
    state.settings.load_icon_mode(IconMode::Unicode);
    let mut metadata = DatabaseMetadata::new("catalog".into());
    metadata.table_summaries = vec![
        TableSummary::new("public".into(), "売上一覧_by_region".into(), None, false)
            .with_kind_info(TableKindInfo {
                kind: TableKind::View,
                ..TableKindInfo::default()
            }),
    ];
    state.session.mark_connected(Arc::new(metadata));
    state.ui.set_explorer_horizontal_offset(usize::MAX);
    let mut terminal = create_test_terminal_sized(80, 24);

    let output = render_to_string(&mut terminal, &mut state);

    assert!(
        output
            .lines()
            .any(|line| line.contains("◇ ") && line.contains("region [view]"))
    );
    assert_eq!(state.ui.explorer_content_width(), 13);
}

#[test]
fn header_marks_the_selected_view_in_each_icon_mode_while_detail_loads() {
    use sabiql_app::model::shared::settings::IconMode;

    let mut state = create_test_state();
    let mut metadata = DatabaseMetadata::new("test_db".into());
    metadata.table_summaries = vec![
        TableSummary::new("public".into(), "active_users".into(), None, false)
            .with_kind_info(table::view_kind_info()),
    ];
    state.session.mark_connected(Arc::new(metadata));
    let _ = state
        .session
        .select_table("public", "active_users", &mut state.query);
    let mut terminal = create_test_terminal_sized(110, 24);

    let headers = IconMode::ALL.map(|mode| {
        state.settings.load_icon_mode(mode);
        let screen = render_to_string(&mut terminal, &mut state);
        screen.lines().next().unwrap().trim_end().to_owned()
    });

    insta::assert_snapshot!(headers.join("\n"));
}
