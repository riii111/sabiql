use super::*;
use harness::{
    TEST_HEIGHT, TEST_WIDTH, connected_state, table_detail_loaded_state, with_current_result,
};
use ratatui::style::{Color, Modifier};
use sabiql::app::model::shared::input_mode::InputMode;
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

    let yellow = Color::Yellow;
    let edit_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find(|&(x, y)| buffer.cell((x, y)).is_some_and(|c| c.fg == yellow));
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
fn modal_border_uses_ansi_darkgray() {
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
        Color::DarkGray,
        "Expected DarkGray fg on modal border at ({}, {}), got {:?}",
        mx,
        my,
        cell.fg
    );
}

// ── Compare tab yank flash ──────────────────────────────────────────────────

fn row_has_flash_bg(buffer: &ratatui::buffer::Buffer, y: u16) -> bool {
    (0..buffer.area.width).any(|x| {
        buffer
            .cell((x, y))
            .is_some_and(|c| c.bg == Theme::YANK_FLASH_BG)
    })
}

fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(""))
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[test]
fn compare_flash_right_only_flashes_plan_text_only() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    let now = test_instant();

    state.modal.set_mode(InputMode::SqlModal);
    state.explain.set_plan(
        "Seq Scan on users  (cost=0.00..10.20 rows=10 width=3273)\n  Filter: email_verified"
            .to_string(),
        false,
        40,
        "SELECT * FROM users WHERE email_verified",
    );
    state.sql_modal.active_tab = SqlModalTab::Compare;
    state.flash_timers.set(FlashId::SqlModal, now);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    for y in 0..TEST_HEIGHT {
        let text = row_text(&buffer, y);
        let has_flash = row_has_flash_bg(&buffer, y);

        // Plan text lines should flash
        if text.contains("Seq Scan") || text.contains("Filter:") {
            assert!(
                has_flash,
                "Row {} should flash: '{}'",
                y,
                &text[..60.min(text.len())]
            );
        }

        // Column headers and chrome should NOT flash
        if text.contains("Previous") || text.contains("Run EXPLAIN") {
            assert!(
                !has_flash,
                "Row {} should NOT flash (chrome): '{}'",
                y,
                &text[..60.min(text.len())]
            );
        }
    }
}

#[test]
fn compare_flash_both_slots_flashes_verdict_and_plan_text() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();
    let now = test_instant();

    state.modal.set_mode(InputMode::SqlModal);
    state.explain.set_plan(
        "Seq Scan on users  (cost=0.00..1000.00 rows=2550 width=36)\n  Filter: (id > 10)"
            .to_string(),
        false,
        100,
        "SELECT * FROM users WHERE id > 10",
    );
    state.explain.set_plan(
        "Index Scan using idx_users_id on users  (cost=0.28..8.30 rows=1 width=36)\n  Index Cond: (id > 10)"
            .to_string(),
        false,
        5,
        "SELECT * FROM users WHERE id > 10",
    );
    state.sql_modal.active_tab = SqlModalTab::Compare;
    state.flash_timers.set(FlashId::SqlModal, now);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let mut flashed = Vec::new();
    let mut not_flashed_chrome = Vec::new();

    for y in 0..TEST_HEIGHT {
        let text = row_text(&buffer, y);
        let has_flash = row_has_flash_bg(&buffer, y);

        // Verdict and plan text should flash
        if text.contains("Improved")
            || text.contains("Total cost")
            || text.contains("Estimated")
            || text.contains("Seq Scan")
            || text.contains("Index Scan")
            || text.contains("Index Cond")
            || text.contains("Filter:")
        {
            assert!(
                has_flash,
                "Row {} should flash: '{}'",
                y,
                &text[..60.min(text.len())]
            );
            flashed.push(y);
        }

        // Column headers should NOT flash
        if text.contains("Previous") && text.contains("Latest") {
            assert!(!has_flash, "Row {} should NOT flash (column header)", y);
            not_flashed_chrome.push(y);
        }
    }

    assert!(
        flashed.len() >= 4,
        "Expected >= 4 flashed rows, got {}",
        flashed.len()
    );
}
