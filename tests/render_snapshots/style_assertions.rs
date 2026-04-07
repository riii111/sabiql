use std::time::Duration;
use std::time::Instant;

use super::*;
use harness::{
    TEST_HEIGHT, TEST_WIDTH, connected_state, create_test_terminal_sized, render_and_get_buffer,
    render_and_get_buffer_at_with_theme, render_and_get_cursor_position, table_detail_loaded_state,
    with_current_result,
};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier};
use sabiql::app::model::app_state::AppState;
use sabiql::app::model::shared::input_mode::InputMode;
use sabiql::app::model::shared::theme_id::ThemeId;
use sabiql::app::model::sql_editor::modal::SqlModalStatus;
use sabiql::app::services::AppServices;
use sabiql::app::update::action::{Action, CursorMove, InputTarget};
use sabiql::app::update::browse::result::reduce_result;
use sabiql::domain::{Column, QueryResult, QuerySource};
use sabiql::ui::theme::{DEFAULT_THEME, TEST_CONTRAST_THEME, ThemePalette};

/// Help modal uses Percentage(70) x Percentage(80), centered in TEST_WIDTH x TEST_HEIGHT.
fn help_modal_origin() -> (u16, u16) {
    let modal_w = TEST_WIDTH * 70 / 100;
    let modal_h = TEST_HEIGHT * 80 / 100;
    let x = (TEST_WIDTH - modal_w) / 2;
    let y = (TEST_HEIGHT - modal_h) / 2;
    (x, y)
}

fn jsonb_detail_state() -> (AppState, Instant) {
    let now = test_instant();
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    let mut table = fixtures::sample_table_detail();
    table.columns.push(Column {
        name: "settings".to_string(),
        data_type: "jsonb".to_string(),
        nullable: true,
        is_primary_key: false,
        is_unique: false,
        default: None,
        comment: None,
        ordinal_position: 4,
    });
    let _ = state.session.set_table_detail(table, 0);
    state.query.set_current_result(Arc::new(QueryResult {
        query: "SELECT id, name, email, settings FROM users LIMIT 100".to_string(),
        columns: vec![
            "id".to_string(),
            "name".to_string(),
            "email".to_string(),
            "settings".to_string(),
        ],
        rows: vec![vec![
            "1".to_string(),
            "Alice".to_string(),
            "alice@example.com".to_string(),
            r#"{"theme":"dark","count":5,"nested":{"enabled":true,"roles":["admin","writer"]}}"#
                .to_string(),
        ]],
        row_count: 1,
        execution_time_ms: 1,
        executed_at: now,
        source: QuerySource::Preview,
        error: None,
        command_tag: None,
    }));
    state.query.pagination.schema = "public".to_string();
    state.query.pagination.table = "users".to_string();
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(0);
    state.result_interaction.enter_cell(3);
    reduce_result(
        &mut state,
        &Action::OpenJsonbDetail,
        &AppServices::stub(),
        now,
    );
    reduce_result(
        &mut state,
        &Action::JsonbEnterEdit,
        &AppServices::stub(),
        now,
    );
    (state, now)
}

#[test]
fn pending_draft_cell_uses_orange_fg() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(1);
    state.result_interaction.enter_cell(2);
    state.modal.set_mode(InputMode::Normal);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .cell_edit_input_mut()
        .set_content("new@example.com".to_string());

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let draft_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|c| c.fg == DEFAULT_THEME.cell_draft_pending_fg)
        });
    assert!(
        draft_cell.is_some(),
        "Expected at least one cell with CELL_DRAFT_PENDING_FG (orange) in the buffer"
    );
}

#[test]
fn active_cell_edit_uses_yellow_fg() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(1);
    state.result_interaction.enter_cell(2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .cell_edit_input_mut()
        .set_content("new@example.com".to_string());

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let edit_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|c| c.fg == DEFAULT_THEME.cell_edit_fg)
        });
    assert!(
        edit_cell.is_some(),
        "Expected at least one cell with CELL_EDIT_FG (yellow) in the buffer"
    );
}

#[test]
fn staged_delete_row_uses_dark_red_bg() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    state.ui.focused_pane = FocusedPane::Result;
    state.result_interaction.enter_row(0);
    state.result_interaction.stage_row(1);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let staged_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|c| c.bg == DEFAULT_THEME.staged_delete_bg)
        });
    assert!(
        staged_cell.is_some(),
        "Expected at least one cell with STAGED_DELETE_BG (dark red) in the buffer"
    );
}

#[test]
fn scrim_applies_dim_modifier() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let cell = buffer.cell((0, 0)).unwrap();
    assert!(
        cell.modifier.contains(Modifier::DIM),
        "Expected DIM modifier on scrim cell (0,0), got {:?}",
        cell.modifier
    );
}

#[test]
fn result_highlight_respects_injected_now() {
    let (mut state, now) = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state, now);
    // Unfocused so highlight border is distinguishable from focus border
    state.ui.focused_pane = FocusedPane::Explorer;

    let highlight_until = now + Duration::from_millis(500);
    state.query.set_result_highlight(highlight_until);

    // Find the Result pane border by searching for "Result" title with Green fg
    let before = now + Duration::from_millis(100);
    let buf_before = render_and_get_buffer_at(&mut terminal, &mut state, before);

    let has_green_border = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            let cell = buf_before.cell((x, y)).unwrap();
            cell.fg == DEFAULT_THEME.highlight_border && cell.symbol() == "─"
        });
    assert!(
        has_green_border,
        "Expected Green border cells when now < highlight_until"
    );

    // now >= highlight_until → no Green border cells
    let after = highlight_until + Duration::from_millis(1);
    let buf_after = render_and_get_buffer_at(&mut terminal, &mut state, after);

    let has_green_border_after = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            let cell = buf_after.cell((x, y)).unwrap();
            cell.fg == DEFAULT_THEME.highlight_border && cell.symbol() == "─"
        });
    assert!(
        !has_green_border_after,
        "Expected no Green border cells when now >= highlight_until"
    );
}

#[test]
fn modal_border_uses_theme_color() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let (mx, my) = help_modal_origin();
    let cell = buffer.cell((mx, my)).unwrap();
    assert_eq!(
        cell.symbol(),
        "╭",
        "Expected '╭' at modal origin ({}, {}), got '{}'",
        mx,
        my,
        cell.symbol()
    );
    assert_eq!(
        cell.fg, DEFAULT_THEME.modal_border,
        "Expected MODAL_BORDER fg on modal border at ({}, {}), got {:?}",
        mx, my, cell.fg
    );
}

#[test]
fn sql_modal_keyword_and_number_use_syntax_colors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor.set_content("SELECT 42".to_string());
    state.sql_modal.set_status(SqlModalStatus::Normal);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let keyword_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            buffer.cell((x, y)).and_then(|cell| {
                (cell.symbol() == "S" && cell.fg == DEFAULT_THEME.sql_keyword).then_some(cell)
            })
        });
    let number_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            buffer.cell((x, y)).and_then(|cell| {
                (cell.symbol() == "4" && cell.fg == DEFAULT_THEME.sql_number).then_some(cell)
            })
        });

    assert!(keyword_cell.is_some(), "Expected a blue SQL keyword cell");
    assert!(
        keyword_cell
            .expect("keyword cell should exist")
            .modifier
            .contains(Modifier::BOLD)
    );
    assert!(number_cell.is_some(), "Expected a yellow SQL number cell");
}

#[test]
fn sql_modal_string_comment_and_operator_use_syntax_colors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("SELECT 'x'::text -- note".to_string());
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let has_string = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == "'" && cell.fg == DEFAULT_THEME.sql_string)
        });
    let has_operator = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == ":" && cell.fg == DEFAULT_THEME.sql_operator)
        });
    let has_comment = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == "-" && cell.fg == DEFAULT_THEME.sql_comment)
        });

    assert!(has_string, "Expected a green SQL string cell");
    assert!(has_operator, "Expected a cyan SQL operator cell");
    assert!(has_comment, "Expected a dark gray SQL comment cell");
}

#[test]
fn sql_modal_normal_and_insert_use_distinct_cursor_styles() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor.set_content("SELECT 1".to_string());
    state.sql_modal.set_status(SqlModalStatus::Normal);

    let normal_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_block_cursor = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            normal_buffer.cell((x, y)).is_some_and(|cell| {
                cell.bg == DEFAULT_THEME.cursor_bg && cell.fg == DEFAULT_THEME.cursor_text_fg
            })
        });

    state.sql_modal.set_status(SqlModalStatus::Editing);

    let insert_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_insert_glyph = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            insert_buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == "\u{258f}")
        });

    assert!(
        has_block_cursor,
        "Expected block cursor styling in SQL normal mode"
    );
    assert!(
        !has_insert_glyph,
        "Expected no fake insert cursor glyph in SQL insert mode"
    );
}

fn sql_modal_block_cursor_position(buffer: &ratatui::buffer::Buffer) -> Option<(u16, u16)> {
    (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| {
            buffer.cell((x, y)).is_some_and(|cell| {
                cell.bg == DEFAULT_THEME.cursor_bg && cell.fg == DEFAULT_THEME.cursor_text_fg
            })
        })
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
        .editor
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.set_status(SqlModalStatus::Normal);

    let head_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let head = sql_modal_block_cursor_position(&head_buffer)
        .expect("Expected block cursor in SQL normal mode at head");

    state
        .sql_modal
        .editor
        .set_content_with_cursor(content.clone(), middle_col);
    let middle_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let middle = sql_modal_block_cursor_position(&middle_buffer)
        .expect("Expected block cursor in SQL normal mode at middle");

    state
        .sql_modal
        .editor
        .set_content_with_cursor(content, tail_col);
    let tail_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let tail = sql_modal_block_cursor_position(&tail_buffer)
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
        .editor
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    state
        .sql_modal
        .editor
        .set_content_with_cursor(content.clone(), middle_col);
    let middle = render_and_get_cursor_position(&mut terminal, &mut state);

    state
        .sql_modal
        .editor
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
        .editor
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    state.sql_modal.editor.set_content_with_cursor(content, 2);
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
        .editor
        .set_content_with_cursor(content.clone(), 0);
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    state.sql_modal.editor.set_content_with_cursor(content, 18);
    let wrapped = render_and_get_cursor_position(&mut terminal, &mut state);

    assert!(wrapped.y > head.y);
}

#[test]
fn jsonb_edit_uses_terminal_cursor_without_fake_glyph() {
    let (mut state, now) = jsonb_detail_state();
    let mut terminal = create_test_terminal();

    let head_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_insert_glyph_at_head = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            head_buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == "\u{258f}")
        });
    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    reduce_result(
        &mut state,
        &Action::TextMoveCursor {
            target: InputTarget::JsonbEdit,
            direction: CursorMove::Right,
        },
        &AppServices::stub(),
        now,
    );

    let moved_buffer = render_and_get_buffer(&mut terminal, &mut state);
    let has_insert_glyph_after_move = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            moved_buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == "\u{258f}")
        });
    let moved = render_and_get_cursor_position(&mut terminal, &mut state);

    assert!(!has_insert_glyph_at_head);
    assert!(!has_insert_glyph_after_move);
    assert_eq!(head.y, moved.y);
    assert_eq!(moved.x, head.x + 1);
}

#[test]
fn jsonb_search_cursor_uses_display_width_for_wide_chars() {
    let (mut state, now) = jsonb_detail_state();
    let mut terminal = create_test_terminal();
    reduce_result(
        &mut state,
        &Action::JsonbExitEdit,
        &AppServices::stub(),
        now,
    );
    reduce_result(
        &mut state,
        &Action::JsonbEnterSearch,
        &AppServices::stub(),
        now,
    );

    let head = render_and_get_cursor_position(&mut terminal, &mut state);

    reduce_result(
        &mut state,
        &Action::TextInput {
            target: InputTarget::JsonbSearch,
            ch: 'a',
        },
        &AppServices::stub(),
        now,
    );
    reduce_result(
        &mut state,
        &Action::TextInput {
            target: InputTarget::JsonbSearch,
            ch: '語',
        },
        &AppServices::stub(),
        now,
    );

    let after_wide = render_and_get_cursor_position(&mut terminal, &mut state);

    assert_eq!(after_wide.y, head.y);
    assert_eq!(after_wide.x, head.x + 3);
}

#[test]
fn sql_modal_unterminated_string_keeps_string_highlight() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("SELECT 'unterminated".to_string());
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let has_string = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer.cell((x, y)).is_some_and(|cell| {
                (cell.symbol() == "'" || cell.symbol() == "u")
                    && cell.fg == DEFAULT_THEME.sql_string
            })
        });

    assert!(
        has_string,
        "Expected unterminated string input to keep SQL string highlight"
    );
}

#[test]
fn sql_modal_unterminated_block_comment_keeps_comment_highlight() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor
        .set_content("SELECT /* pending".to_string());
    state.sql_modal.set_status(SqlModalStatus::Editing);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let has_comment = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer.cell((x, y)).is_some_and(|cell| {
                (cell.symbol() == "/" || cell.symbol() == "*")
                    && cell.fg == DEFAULT_THEME.sql_comment
            })
        });

    assert!(
        has_comment,
        "Expected unterminated block comment input to keep SQL comment highlight"
    );
}

#[test]
fn injected_palette_changes_shell_modal_and_picker_styles() {
    let (mut state, now) = connected_state();
    let mut terminal = create_test_terminal();
    let theme = ThemePalette {
        focus_border: Color::Rgb(0x11, 0x88, 0xdd),
        modal_border: Color::Rgb(0xdd, 0x44, 0x11),
        completion_selected_bg: Color::Rgb(0x22, 0x66, 0x33),
        modal_hint: Color::Rgb(0xaa, 0xee, 0x22),
        ..DEFAULT_THEME
    };

    state.ui.focused_pane = FocusedPane::Explorer;
    let shell_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let has_custom_focus_border = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            shell_buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == "─" && cell.fg == theme.focus_border)
        });
    assert!(
        has_custom_focus_border,
        "Expected shell border to use injected focus border color"
    );

    state.modal.set_mode(InputMode::Help);
    let help_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let (mx, my) = help_modal_origin();
    let modal_corner = help_buffer.cell((mx, my)).unwrap();
    assert_eq!(modal_corner.fg, theme.modal_border);
    let has_custom_help_hint = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            help_buffer
                .cell((x, y))
                .is_some_and(|cell| cell.fg == theme.modal_hint)
        });
    assert!(
        has_custom_help_hint,
        "Expected shared modal hint to use injected hint color"
    );

    state.modal.set_mode(InputMode::CommandPalette);
    let picker_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let has_custom_picker_selection = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            picker_buffer
                .cell((x, y))
                .is_some_and(|cell| cell.bg == theme.completion_selected_bg)
        });
    assert!(
        has_custom_picker_selection,
        "Expected picker selection to use injected selected background"
    );

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.set_status(SqlModalStatus::Normal);
    let sql_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let has_custom_sql_hint = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            sql_buffer
                .cell((x, y))
                .is_some_and(|cell| cell.fg == theme.modal_hint)
        });
    assert!(
        has_custom_sql_hint,
        "Expected SQL modal hint to use injected hint color"
    );
}

#[test]
fn state_theme_id_drives_render_palette_resolution() {
    let (mut state, _now) = connected_state();
    let mut terminal = create_test_terminal();

    state.ui.set_theme(ThemeId::TestContrast);
    state.ui.focused_pane = FocusedPane::Explorer;

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let has_test_theme_focus_border = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer.cell((x, y)).is_some_and(|cell| {
                cell.symbol() == "─" && cell.fg == TEST_CONTRAST_THEME.focus_border
            })
        });

    assert!(
        has_test_theme_focus_border,
        "Expected render() to resolve palette from state.theme_id"
    );
}

#[test]
fn sql_completion_popup_uses_injected_theme_styles() {
    let (mut state, now) = connected_state();
    let mut terminal = create_test_terminal();
    let theme = ThemePalette {
        modal_border: Color::Rgb(0xdd, 0x44, 0x11),
        completion_selected_bg: Color::Rgb(0x22, 0x66, 0x33),
        ..DEFAULT_THEME
    };

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor.set_content("SELECT ".to_string());
    state.sql_modal.set_status(SqlModalStatus::Editing);
    state.sql_modal.completion.visible = true;
    state.sql_modal.completion.selected_index = 0;
    state.sql_modal.completion.candidates = vec![
        CompletionCandidate {
            text: "users".into(),
            kind: CompletionKind::Table,
            score: 100,
        },
        CompletionCandidate {
            text: "posts".into(),
            kind: CompletionKind::Table,
            score: 90,
        },
    ];

    let buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);

    let frame_area = Rect::new(0, 0, TEST_WIDTH, TEST_HEIGHT);
    let [modal_area] = Layout::horizontal([Constraint::Percentage(80)])
        .flex(Flex::Center)
        .areas(frame_area);
    let [modal_area] = Layout::vertical([Constraint::Percentage(60)])
        .flex(Flex::Center)
        .areas(modal_area);
    let inner_area = Rect::new(
        modal_area.x + 1,
        modal_area.y + 1,
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    let content_area = Rect {
        x: inner_area.x + 1,
        width: inner_area.width.saturating_sub(2),
        ..inner_area
    };
    let [editor_area, _, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(content_area);
    let (cursor_row, cursor_col) = state.sql_modal.editor.cursor_to_position();
    let cursor_col = cursor_col as u16;
    let cursor_row = cursor_row as u16;
    let scroll_row = state.sql_modal.editor.scroll_row() as u16;
    let visible_count = state.sql_modal.completion.candidates.len().min(8) as u16;
    let popup_height = visible_count + 2;
    let popup_width = 45u16.min(modal_area.width);
    let popup_x = if modal_area.width < popup_width {
        modal_area.x
    } else {
        (editor_area.x + cursor_col).min(modal_area.right().saturating_sub(popup_width))
    };
    let visible_row = cursor_row.saturating_sub(scroll_row);
    let cursor_screen_y = editor_area.y + visible_row;
    let popup_y = if cursor_screen_y + 1 + popup_height > modal_area.bottom() {
        cursor_screen_y.saturating_sub(popup_height)
    } else {
        cursor_screen_y + 1
    };

    let has_selected_completion = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.bg == theme.completion_selected_bg)
        });
    let top_left = buffer.cell((popup_x, popup_y)).unwrap();
    let top_right = buffer.cell((popup_x + popup_width - 1, popup_y)).unwrap();
    let has_completion_border = top_left.symbol() == "┌"
        && top_left.fg == theme.modal_border
        && top_right.symbol() == "┐"
        && top_right.fg == theme.modal_border;

    assert!(
        has_completion_border,
        "Expected anchored completion popup border to use injected modal border color"
    );
    assert!(
        has_selected_completion,
        "Expected completion popup selection to use injected selected background"
    );
}
