use super::*;
use harness::{
    explorer_selected_state, render_and_get_buffer, table_detail_loaded_state, with_current_result,
};
use sabiql_app::model::shared::ui_state::FocusMode;
use sabiql_domain::{
    ConnectionId, DatabaseMetadata, Schema, TableKind, TableKindInfo, TableSummary,
};
use sabiql_ui::theme::DEFAULT_THEME;

fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (buffer.area.left()..buffer.area.right())
        .filter_map(|x| buffer.cell((x, y)))
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

fn assert_row_text_color(
    buffer: &ratatui::buffer::Buffer,
    y: u16,
    text: &str,
    expected: ratatui::style::Color,
) {
    let row = row_text(buffer, y);
    let start = row
        .find(text)
        .map(|byte_offset| buffer.area.left() + row[..byte_offset].chars().count() as u16)
        .expect("expected text in target row");
    for (offset, _) in text.chars().enumerate() {
        assert_eq!(
            buffer.cell((start + offset as u16, y)).unwrap().fg,
            expected
        );
    }
}

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

    let buffer = render_and_get_buffer(&mut terminal, &mut state);
    let footer = row_text(&buffer, 49);
    assert!(footer.contains("Connection failed: timeout"));
    assert_row_text_color(
        &buffer,
        49,
        "Connection failed: timeout",
        DEFAULT_THEME.semantic.status.error,
    );
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
    let buffer = render_and_get_buffer(&mut terminal, &mut state);
    let explorer_rows = (2..6).map(|y| row_text(&buffer, y)).collect::<Vec<_>>();
    assert!(explorer_rows[0].contains("> users"));
    assert!(explorer_rows[1].contains("settings"));
    assert!(explorer_rows[2].contains("notes_fts"));
    assert!(explorer_rows[3].contains("typed_users"));
    assert!(explorer_rows.iter().all(|row| !row.contains("main.")));
    assert!(
        explorer_rows
            .iter()
            .all(|row| !row.contains("WITHOUT ROWID"))
    );
    assert!(explorer_rows.iter().all(|row| !row.contains("virtual")));
    assert!(explorer_rows.iter().all(|row| !row.contains("strict")));
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
    let buffer = render_and_get_buffer(&mut terminal, &mut state);
    let header = row_text(&buffer, 0);
    assert!(header.contains("app.db ▸ users"));
    assert!(!header.contains("main.users"));
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
    let buffer = render_and_get_buffer(&mut terminal, &mut state);
    let header = row_text(&buffer, 0);
    let explorer_rows = [row_text(&buffer, 2), row_text(&buffer, 3)];
    assert!(header.contains("app ▸ users"));
    assert!(explorer_rows[0].contains("users"));
    assert!(explorer_rows[1].contains("audit_log"));
    assert!(explorer_rows.iter().all(|row| !row.contains("app.")));
}
