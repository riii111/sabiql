use std::time::Duration;
use std::time::Instant;

use super::*;
use harness::{
    TEST_HEIGHT, TEST_WIDTH, connected_state, render_and_get_buffer,
    render_and_get_buffer_at_with_theme, render_and_get_cursor_position, table_detail_loaded_state,
    with_current_result,
};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier};
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::shared::input_mode::InputMode;
use sabiql_app::model::shared::theme_id::ThemeId;
use sabiql_app::model::shared::ui_state::{HELP_MODAL_HEIGHT_PERCENT, HELP_MODAL_WIDTH_PERCENT};
use sabiql_app::update::action::{Action, CursorMove, InputTarget, ModalKind};
use sabiql_app::update::browse::result::dispatch_result;
use sabiql_domain::{Column, ConnectionId, QueryResult};
use sabiql_ui::theme::test_support::TEST_CONTRAST_THEME;
use sabiql_ui::theme::{
    ComponentTokens, DEFAULT_THEME, EditorTokens, LIGHT_THEME, ModalTokens, SemanticTokens,
    SurfaceTokens, ThemePalette,
};

fn has_cell(buffer: &Buffer, predicate: impl Fn(&Cell) -> bool) -> bool {
    for y in buffer.area.top()..buffer.area.bottom() {
        for x in buffer.area.left()..buffer.area.right() {
            if buffer.cell((x, y)).is_some_and(&predicate) {
                return true;
            }
        }
    }
    false
}

fn has_cell_in_area(buffer: &Buffer, area: Rect, predicate: impl Fn(&Cell) -> bool) -> bool {
    for y in area.top().max(buffer.area.top())..area.bottom().min(buffer.area.bottom()) {
        for x in area.left().max(buffer.area.left())..area.right().min(buffer.area.right()) {
            if buffer.cell((x, y)).is_some_and(&predicate) {
                return true;
            }
        }
    }
    false
}

fn help_modal_area() -> Rect {
    let modal_w = TEST_WIDTH * HELP_MODAL_WIDTH_PERCENT / 100;
    let modal_h = TEST_HEIGHT * HELP_MODAL_HEIGHT_PERCENT / 100;
    let x = (TEST_WIDTH - modal_w) / 2;
    let y = (TEST_HEIGHT - modal_h) / 2;
    Rect::new(x, y, modal_w, modal_h)
}

fn help_modal_origin() -> (u16, u16) {
    let area = help_modal_area();
    (area.x, area.y)
}

fn find_row0_text_start(buffer: &ratatui::buffer::Buffer, text: &str) -> Option<u16> {
    let len = text.chars().count() as u16;
    if len == 0 || len > TEST_WIDTH {
        return None;
    }
    (0..=TEST_WIDTH - len).find(|&x| {
        text.chars().enumerate().all(|(i, c)| {
            buffer
                .cell((x + i as u16, 0))
                .and_then(|cell| cell.symbol().chars().next())
                == Some(c)
        })
    })
}

fn assert_header_status_color(buffer: &ratatui::buffer::Buffer, text: &str, expected: Color) {
    let start = find_row0_text_start(buffer, text)
        .unwrap_or_else(|| panic!("Expected header status text {text:?} to be rendered"));
    for (i, c) in text.chars().enumerate() {
        let cell = buffer
            .cell((start + i as u16, 0))
            .expect("status text cell in row 0");
        assert_eq!(
            cell.fg,
            expected,
            "Status text {text:?}: cell at ({}, 0) rendering '{c}' should have fg={expected:?}",
            start + i as u16
        );
    }
}

mod completion_popup;
mod cursor_geometry;
mod sql_syntax;
mod style_colors;
mod theme_injection;
