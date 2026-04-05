use std::time::Duration;

use super::*;
use harness::{
    TEST_HEIGHT, TEST_WIDTH, connected_state, render_and_get_buffer_at_with_theme,
    table_detail_loaded_state, with_current_result,
};
use ratatui::style::{Color, Modifier};
use sabiql::app::model::shared::input_mode::InputMode;
use sabiql::app::model::sql_editor::modal::SqlModalStatus;
use sabiql::ui::theme::{DEFAULT_THEME, ThemePalette};

/// Help modal uses Percentage(70) x Percentage(80), centered in TEST_WIDTH x TEST_HEIGHT.
fn help_modal_origin() -> (u16, u16) {
    let modal_w = TEST_WIDTH * 70 / 100;
    let modal_h = TEST_HEIGHT * 80 / 100;
    let x = (TEST_WIDTH - modal_w) / 2;
    let y = (TEST_HEIGHT - modal_h) / 2;
    (x, y)
}

fn sql_modal_origin() -> (u16, u16, u16, u16) {
    let modal_w = TEST_WIDTH * 80 / 100;
    let modal_h = TEST_HEIGHT * 60 / 100;
    let x = (TEST_WIDTH - modal_w) / 2;
    let y = (TEST_HEIGHT - modal_h) / 2;
    (x, y, modal_w, modal_h)
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
    let help_modal_height = TEST_HEIGHT * 80 / 100;
    let help_hint_row = my + help_modal_height.saturating_sub(1);
    let has_custom_help_hint = (mx..TEST_WIDTH).any(|x| {
        help_buffer
            .cell((x, help_hint_row))
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
    let (sql_x, sql_y, sql_w, sql_h) = sql_modal_origin();
    let sql_hint_row = sql_y + sql_h.saturating_sub(1);
    let has_custom_sql_hint = (sql_x..sql_x + sql_w).any(|x| {
        sql_buffer
            .cell((x, sql_hint_row))
            .is_some_and(|cell| cell.fg == theme.modal_hint)
    });
    assert!(
        has_custom_sql_hint,
        "Expected SQL modal hint to use injected hint color"
    );
}
