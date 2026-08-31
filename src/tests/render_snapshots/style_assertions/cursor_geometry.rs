use super::*;
use crate::tests::harness::{focus_connection_field, set_connection_input};

fn json_detail_state() -> (AppState, Instant) {
    let now = test_instant();
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata()));
    let mut table = fixtures::sample_postgres_table_detail();
    table.columns.push(Column {
        name: "settings".to_string(),
        data_type: "jsonb".to_string(),
        attributes: ColumnAttributes::NULLABLE,
        default: None,
        comment: None,
        ordinal_position: 4,
        character_set_name: None,
        collation_name: None,
        generation_expression: None,
        generation_kind: None,
    });
    let _ = state.session.set_table_detail(table, 0);
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT id, name, email, settings FROM users LIMIT 100".to_string(),
            vec![
                "id".to_string(),
                "name".to_string(),
                "email".to_string(),
                "settings".to_string(),
            ],
            vec![vec![
                "1".to_string(),
                "Alice".to_string(),
                "alice@example.com".to_string(),
                r#"{"theme":"dark","count":5,"nested":{"enabled":true,"roles":["admin","writer"]}}"#
                    .to_string(),
            ]],
            1,
            QuerySource::Preview,
        )));
    state.query.pagination.reset_for_table("public", "users");
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 3);
    dispatch_result(
        &mut state,
        &Action::OpenModal(ModalKind::JsonDetail),
        &AppServices::stub(),
        now,
    );
    dispatch_result(
        &mut state,
        &Action::JsonEnterEdit,
        &AppServices::stub(),
        now,
    );
    (state, now)
}
fn block_cursor_position(buffer: &ratatui::buffer::Buffer) -> Option<(u16, u16)> {
    (buffer.area.top()..buffer.area.bottom())
        .flat_map(|y| (buffer.area.left()..buffer.area.right()).map(move |x| (x, y)))
        .find(|&(x, y)| {
            buffer.cell((x, y)).is_some_and(|cell| {
                cell.bg == DEFAULT_THEME.semantic.cursor.bg
                    && cell.fg == DEFAULT_THEME.semantic.cursor.text_fg
            })
        })
}

#[test]
fn sql_modal_normal_and_insert_use_distinct_cursor_styles() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT 1".to_string());
    state.sql_modal.enter_normal();

    let normal_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_block_cursor = has_cell(&normal_buffer, |cell| {
        cell.bg == DEFAULT_THEME.semantic.cursor.bg
            && cell.fg == DEFAULT_THEME.semantic.cursor.text_fg
    });

    state.sql_modal.enter_editing();

    let insert_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_insert_glyph = has_cell(&insert_buffer, |cell| cell.symbol() == "\u{258f}");

    assert!(
        has_block_cursor,
        "Expected block cursor styling in SQL normal mode"
    );
    assert!(
        !has_insert_glyph,
        "Expected no fake insert cursor glyph in SQL insert mode"
    );
}

#[test]
fn help_uses_block_cursor_while_browsing_and_terminal_cursor_while_filtering() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);

    let browsing_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let modal_area = help_modal_area();
    let has_block_cursor = has_cell_in_area(&browsing_buffer, modal_area, |cell| {
        cell.bg == DEFAULT_THEME.semantic.cursor.bg
            && cell.fg == DEFAULT_THEME.semantic.cursor.text_fg
    });

    state.ui.help_mut().enter_filter_editing();

    let filtering_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_block_cursor_while_filtering =
        has_cell_in_area(&filtering_buffer, modal_area, |cell| {
            cell.bg == DEFAULT_THEME.semantic.cursor.bg
                && cell.fg == DEFAULT_THEME.semantic.cursor.text_fg
        });
    let terminal_cursor = render_and_get_cursor_position(&mut terminal, &mut state);

    assert!(
        has_block_cursor,
        "Expected block cursor while browsing help"
    );
    assert!(
        !has_block_cursor_while_filtering,
        "Expected no block cursor while filtering help"
    );
    assert!(
        terminal_cursor.x >= modal_area.left()
            && terminal_cursor.x < modal_area.right()
            && terminal_cursor.y >= modal_area.top()
            && terminal_cursor.y < modal_area.bottom(),
        "Expected terminal cursor inside the help filter"
    );
}

#[test]
fn sql_modal_normal_cursor_position_tracks_head_middle_and_tail() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    let content = "SELECT 1".to_string();
    let middle_col = 4;
    let tail_col = content.chars().count();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.enter_normal();

    let head_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let head = block_cursor_position(&head_buffer)
        .expect("Expected block cursor in SQL normal mode at head");

    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content.clone(), middle_col);
    let middle_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let middle = block_cursor_position(&middle_buffer)
        .expect("Expected block cursor in SQL normal mode at middle");

    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content, tail_col);
    let tail_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let tail = block_cursor_position(&tail_buffer)
        .expect("Expected block cursor in SQL normal mode at tail");

    assert_eq!(
        head.1, middle.1,
        "Expected head and middle cursor on the same row"
    );
    assert_eq!(
        middle.1, tail.1,
        "Expected middle and tail cursor on the same row"
    );
    assert_eq!(middle.0, head.0 + middle_col as u16);
    assert_eq!(tail.0, head.0 + tail_col as u16);
}

#[test]
fn sql_modal_insert_cursor_position_tracks_head_middle_and_tail() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    let content = "SELECT 1".to_string();
    let middle_col = 4;
    let tail_col = content.chars().count();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.enter_editing();

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content.clone(), middle_col);
    let middle = render_and_get_cursor_position(&mut terminal, &mut state);

    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content, tail_col);
    let tail = render_and_get_cursor_position(&mut terminal, &mut state);

    assert_eq!(head.y, middle.y);
    assert_eq!(middle.y, tail.y);
    assert_eq!(middle.x, head.x + middle_col as u16);
    assert_eq!(tail.x, head.x + tail_col as u16);
}

#[test]
fn sql_modal_insert_cursor_uses_display_width_for_wide_chars() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    let content = "a語b".to_string();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.enter_editing();

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content, 2);
    let after_wide = render_and_get_cursor_position(&mut terminal, &mut state);

    assert_eq!(after_wide.y, head.y);
    assert_eq!(after_wide.x, head.x + 3);
}

#[test]
fn sql_modal_insert_cursor_advances_visual_row_when_line_wraps() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal_sized(24, TEST_HEIGHT);
    let content = "12345678901234567890".to_string();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.enter_editing();

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    state
        .sql_modal
        .editor_mut_for_input()
        .set_content_with_cursor(content, 18);
    let wrapped = render_and_get_cursor_position(&mut terminal, &mut state);

    assert!(wrapped.y > head.y);
}

#[test]
fn connection_setup_cursor_matrix() {
    for (cursor, expected_symbol) in [(0, "d"), (7, "p"), (14, " ")] {
        let mut state = create_test_state();
        let mut terminal = create_test_terminal();

        state.modal.set_mode(InputMode::ConnectionSetup);
        focus_connection_field(&mut state, ConnectionField::Host);
        set_connection_input(
            &mut state,
            ConnectionField::Host,
            TextInputState::new("db.example.com", cursor),
        );

        let buffer = render_and_get_buffer(&mut terminal, &mut state);
        let (x, y) = block_cursor_position(&buffer).expect("expected connection cursor");
        let cell = buffer.cell((x, y)).expect("connection cursor cell");

        assert_eq!(cell.symbol(), expected_symbol);
    }
}

#[test]
fn cell_edit_cursor_matrix() {
    for (cursor, expected_symbol) in [(0, "a"), (7, "h"), (14, "o")] {
        let mut state = table_detail_loaded_state();
        let mut terminal = create_test_terminal();

        with_current_result(&mut state);
        state.ui.set_focused_pane(FocusedPane::Result);
        state.result_interaction.activate_cell(1, 2);
        state.modal.set_mode(InputMode::CellEdit);
        state
            .result_interaction
            .begin_cell_edit(1, 2, "abcdefghijklmno".to_string());
        state.result_interaction.cell_edit_set_cursor(cursor);

        let buffer = render_and_get_buffer(&mut terminal, &mut state);
        let (x, y) = block_cursor_position(&buffer).expect("expected cell edit cursor");
        let cell = buffer.cell((x, y)).expect("cell edit cursor cell");

        assert_eq!(cell.symbol(), expected_symbol);
    }
}

#[test]
fn json_edit_uses_terminal_cursor_without_fake_glyph() {
    let (mut state, now) = json_detail_state();
    let mut terminal = create_test_terminal();

    let head_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_insert_glyph_at_head = has_cell(&head_buffer, |cell| cell.symbol() == "\u{258f}");
    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    dispatch_result(
        &mut state,
        &Action::TextMoveCursor {
            target: InputTarget::JsonEdit,
            direction: CursorMove::Right,
        },
        &AppServices::stub(),
        now,
    );

    let moved_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_insert_glyph_after_move = has_cell(&moved_buffer, |cell| cell.symbol() == "\u{258f}");
    let moved = render_and_get_cursor_position(&mut terminal, &mut state);

    assert!(!has_insert_glyph_at_head);
    assert!(!has_insert_glyph_after_move);
    assert_eq!(head.y, moved.y);
    assert_eq!(moved.x, head.x + 1);
}

#[test]
fn json_search_cursor_uses_display_width_for_wide_chars() {
    let (mut state, now) = json_detail_state();
    let mut terminal = create_test_terminal();
    dispatch_result(&mut state, &Action::JsonExitEdit, &AppServices::stub(), now);
    dispatch_result(
        &mut state,
        &Action::JsonEnterSearch,
        &AppServices::stub(),
        now,
    );

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    dispatch_result(
        &mut state,
        &Action::TextInput {
            target: InputTarget::JsonSearch,
            ch: 'a',
        },
        &AppServices::stub(),
        now,
    );
    dispatch_result(
        &mut state,
        &Action::TextInput {
            target: InputTarget::JsonSearch,
            ch: '語',
        },
        &AppServices::stub(),
        now,
    );

    let after_wide = render_and_get_cursor_position(&mut terminal, &mut state);

    assert_eq!(after_wide.y, head.y);
    assert_eq!(after_wide.x, head.x + 3);
}
