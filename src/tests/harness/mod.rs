pub mod fixtures;

use std::sync::Arc;
use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;

use app::model::app_state::AppState;
use app::model::connection::setup::ConnectionField;
use app::model::shared::db_capabilities::DbCapabilities;
use app::model::shared::text_input::TextInputState;
use app::services::AppServices;
use ui::shell::layout::MainLayout;
use ui::theme::{ThemePalette, palette_for};

pub const TEST_WIDTH: u16 = 80;
pub const TEST_HEIGHT: u16 = 24;

pub fn test_instant() -> Instant {
    Instant::now()
}

pub fn create_test_state() -> AppState {
    let mut state = AppState::new("test_project".to_string());
    state
        .session
        .set_active_connection_name_for_test(Some("localhost:5432/test".to_string()));
    state
        .session
        .set_active_db_capabilities_for_test(DbCapabilities::postgres_like());
    state
}

pub fn focus_connection_field(state: &mut AppState, field: ConnectionField) {
    let fields = state.connection_setup.visible_fields();
    let target_idx = fields
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap_or_else(|| panic!("field {field:?} is not visible: {fields:?}"));

    loop {
        let current = state.connection_setup.focused_field();
        if current == field {
            return;
        }
        let current_idx = fields
            .iter()
            .position(|candidate| *candidate == current)
            .expect("focused field must be visible");
        if target_idx > current_idx {
            state.connection_setup.focus_next_field();
        } else {
            state.connection_setup.focus_prev_field();
        }
    }
}

pub fn set_connection_input(state: &mut AppState, field: ConnectionField, input: TextInputState) {
    *state
        .connection_setup
        .input_mut(field)
        .expect("expected text input field") = input;
}

pub fn create_test_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(TEST_WIDTH, TEST_HEIGHT);
    Terminal::new(backend).unwrap()
}

pub fn create_test_terminal_sized(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).unwrap()
}

const FIXED_TIME_MS: u128 = 0;

pub fn render_and_get_buffer(terminal: &mut Terminal<TestBackend>, state: &mut AppState) -> Buffer {
    render_and_get_buffer_at(terminal, state, test_instant())
}

pub fn render_and_get_buffer_at(
    terminal: &mut Terminal<TestBackend>,
    state: &mut AppState,
    now: Instant,
) -> Buffer {
    render_and_get_buffer_at_with_theme(terminal, state, now, palette_for(state.ui.theme_id()))
}

pub fn render_and_get_buffer_at_with_theme(
    terminal: &mut Terminal<TestBackend>,
    state: &mut AppState,
    now: Instant,
    theme: &ThemePalette,
) -> Buffer {
    terminal
        .draw(|frame| {
            let output = MainLayout::render_with_theme(
                frame,
                state,
                Some(FIXED_TIME_MS),
                &AppServices::stub(),
                now,
                theme,
            );
            state
                .ui
                .set_inspector_viewport_plan(output.inspector_viewport_plan);
            state
                .ui
                .set_result_viewport_plan(output.result_viewport_plan);
            state.ui.set_result_widths_cache(output.result_widths_cache);
            state
                .ui
                .set_inspector_pane_height(output.inspector_pane_height);
            state.ui.set_result_pane_height(output.result_pane_height);
        })
        .unwrap();

    terminal.backend().buffer().clone()
}

pub fn render_to_string(terminal: &mut Terminal<TestBackend>, state: &mut AppState) -> String {
    let buffer = render_and_get_buffer(terminal, state);
    buffer_to_string(&buffer)
}

pub fn render_and_get_cursor_position(
    terminal: &mut Terminal<TestBackend>,
    state: &mut AppState,
) -> Position {
    let _ = render_and_get_buffer(terminal, state);
    terminal.backend_mut().get_cursor_position().unwrap()
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut result = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = buffer.cell((x, y)).unwrap();
            result.push_str(cell.symbol());
        }
        if y < buffer.area.height - 1 {
            result.push('\n');
        }
    }
    result
}

pub fn connected_state() -> (AppState, Instant) {
    let now = test_instant();
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata(now)));
    (state, now)
}

pub fn explorer_selected_state() -> (AppState, Instant) {
    let (mut state, now) = connected_state();
    state.ui.set_explorer_selection(Some(0));
    (state, now)
}

pub fn table_detail_loaded_state() -> (AppState, Instant) {
    let (mut state, now) = explorer_selected_state();
    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);
    (state, now)
}

pub fn with_current_result(state: &mut AppState, now: Instant) {
    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result(now)));
}
