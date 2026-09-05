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
    state.session.activate_connection_with_dsn(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        DatabaseType::SQLite,
        "sqlite:///tmp/app.db",
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
    state.session.activate_connection_with_dsn(
        &ConnectionId::from_string("sqlite-test"),
        "app.db",
        DatabaseType::SQLite,
        "sqlite:///tmp/app.db",
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
    state.session.activate_connection_with_target(
        &ConnectionId::from_string("mysql-test"),
        "mysql",
        DatabaseType::MySQL,
        "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
        Some("app"),
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
            .set_table_detail(fixtures::minimal_table("app", "users"), generation)
    );
    state.ui.set_explorer_selection(Some(0));

    let mut terminal = create_test_terminal();
    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
