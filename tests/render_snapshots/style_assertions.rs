use std::time::Duration;

use super::*;
use harness::{
    TEST_HEIGHT, TEST_WIDTH, connected_state, table_detail_loaded_state, with_current_result,
};
use ratatui::style::{Color, Modifier};
use sabiql::app::model::shared::input_mode::InputMode;
use sabiql::app::model::sql_editor::modal::SqlModalStatus;
use sabiql::ui::theme::Theme;

/// Help modal uses Percentage(70) x Percentage(80), centered in TEST_WIDTH x TEST_HEIGHT.
fn help_modal_origin() -> (u16, u16) {
    let modal_w = TEST_WIDTH * 70 / 100;
    let modal_h = TEST_HEIGHT * 80 / 100;
    let x = (TEST_WIDTH - modal_w) / 2;
    let y = (TEST_HEIGHT - modal_h) / 2;
    (x, y)
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

    let orange = Color::Rgb(0xff, 0x99, 0x00);
    let draft_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| buffer.cell((x, y)).is_some_and(|c| c.fg == orange));
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
                .is_some_and(|c| c.fg == Theme::CELL_EDIT_FG)
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

    let dark_red = Color::Rgb(0x3d, 0x22, 0x22);
    let staged_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| buffer.cell((x, y)).is_some_and(|c| c.bg == dark_red));
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
            cell.fg == Theme::HIGHLIGHT_BORDER && cell.symbol() == "─"
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
            cell.fg == Theme::HIGHLIGHT_BORDER && cell.symbol() == "─"
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
        cell.fg,
        Theme::MODAL_BORDER,
        "Expected MODAL_BORDER fg on modal border at ({}, {}), got {:?}",
        mx,
        my,
        cell.fg
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
                (cell.symbol() == "S" && cell.fg == Theme::SQL_KEYWORD).then_some(cell)
            })
        });
    let number_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            buffer.cell((x, y)).and_then(|cell| {
                (cell.symbol() == "4" && cell.fg == Theme::SQL_NUMBER).then_some(cell)
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
                .is_some_and(|cell| cell.symbol() == "'" && cell.fg == Theme::SQL_STRING)
        });
    let has_operator = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == ":" && cell.fg == Theme::SQL_OPERATOR)
        });
    let has_comment = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .any(|(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == "-" && cell.fg == Theme::SQL_COMMENT)
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
                (cell.symbol() == "'" || cell.symbol() == "u") && cell.fg == Theme::SQL_STRING
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
                (cell.symbol() == "/" || cell.symbol() == "*") && cell.fg == Theme::SQL_COMMENT
            })
        });

    assert!(
        has_comment,
        "Expected unterminated block comment input to keep SQL comment highlight"
    );
}
