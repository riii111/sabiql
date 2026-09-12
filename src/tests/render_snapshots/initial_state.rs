use super::*;
use crate::tests::harness::render_and_get_buffer;
use sabiql_domain::ConnectionId;
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
fn initial_state_no_metadata() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn explorer_shows_retry_when_metadata_reload_fails() {
    let mut state = postgres_connected_state();
    state.session.mark_connection_failed();
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);
    let rows = output.lines().collect::<Vec<_>>();
    assert!(rows[2].contains("Metadata load failed"));
    assert!(rows[3].contains("r: retry, Enter: details"));
    assert!(!rows[2].contains("public."));
}

#[test]
fn explorer_shows_not_connected_when_no_active_connection() {
    let mut state = create_test_state();
    state.session.clear_connection();
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);
    let rows = output.lines().collect::<Vec<_>>();
    assert!(rows[2].contains("Press 'c' to select a connection"));
    assert!(!rows[2].contains("public."));
}

#[test]
fn header_shows_effective_user_at_normal_width() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "test",
        DatabaseType::PostgreSQL,
        "postgresql://localhost/test",
    );
    let metadata = state.session.metadata().cloned().expect("metadata");
    state
        .session
        .mark_connected_with_user(Arc::new(metadata), Some("app_user".to_string()));
    let mut terminal = create_test_terminal();

    let buffer = render_and_get_buffer(&mut terminal, &mut state);
    let header = row_text(&buffer, 0);
    assert!(header.contains("user: app_user"));
    assert_row_text_color(
        &buffer,
        0,
        "user: app_user",
        DEFAULT_THEME.semantic.text.secondary,
    );
}

#[test]
fn mysql_header_shows_effective_user_without_dsn_password() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "test",
        DatabaseType::MySQL,
        "mysql://app:header-secret@localhost/app",
    );
    let metadata = state.session.metadata().cloned().expect("metadata");
    state
        .session
        .mark_connected_with_user(Arc::new(metadata), Some("app@%".to_string()));
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    assert!(output.contains("user: app@%"));
    assert!(!output.contains("header-secret"));
}

#[test]
fn header_truncates_connection_name_at_narrow_width() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "very-long-connection-name-that-must-yield-space-to-the-user",
        DatabaseType::PostgreSQL,
        "postgresql://localhost/test",
    );
    let metadata = state.session.metadata().cloned().expect("metadata");
    state
        .session
        .mark_connected_with_user(Arc::new(metadata), Some("app_user".to_string()));
    let mut terminal = create_test_terminal_sized(60, 20);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn header_shows_read_only_badge_at_narrow_width() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "test",
        DatabaseType::PostgreSQL,
        "postgresql://localhost/test",
    );
    state.session.enable_read_only();
    let metadata = state.session.metadata().cloned().expect("metadata");
    state
        .session
        .mark_connected_with_user(Arc::new(metadata), Some("app_user".to_string()));
    let mut terminal = create_test_terminal_sized(80, 20);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
