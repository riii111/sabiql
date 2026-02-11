pub mod fixtures;

use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use sabiql::app::state::AppState;
use sabiql::ui::components::layout::MainLayout;

pub const TEST_WIDTH: u16 = 80;
pub const TEST_HEIGHT: u16 = 24;

pub fn test_instant() -> Instant {
    Instant::now()
}

pub fn create_test_state() -> AppState {
    let mut state = AppState::new("test_project".to_string());
    // Set a default connection name for consistent test snapshots
    state.runtime.active_connection_name = Some("localhost:5432/test".to_string());
    state
}

pub fn create_test_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(TEST_WIDTH, TEST_HEIGHT);
    Terminal::new(backend).unwrap()
}

const FIXED_TIME_MS: u128 = 0;

pub fn render_and_get_buffer(terminal: &mut Terminal<TestBackend>, state: &mut AppState) -> Buffer {
    terminal
        .draw(|frame| {
            let output = MainLayout::render(frame, state, Some(FIXED_TIME_MS));
            state.ui.inspector_viewport_plan = output.inspector_viewport_plan;
            state.ui.result_viewport_plan = output.result_viewport_plan;
            state.ui.inspector_pane_height = output.inspector_pane_height;
            state.ui.result_pane_height = output.result_pane_height;
        })
        .unwrap();

    terminal.backend().buffer().clone()
}

pub fn render_to_string(terminal: &mut Terminal<TestBackend>, state: &mut AppState) -> String {
    terminal
        .draw(|frame| {
            let output = MainLayout::render(frame, state, Some(FIXED_TIME_MS));
            state.ui.inspector_viewport_plan = output.inspector_viewport_plan;
            state.ui.result_viewport_plan = output.result_viewport_plan;
            state.ui.inspector_pane_height = output.inspector_pane_height;
            state.ui.result_pane_height = output.result_pane_height;
        })
        .unwrap();

    buffer_to_string(terminal.backend().buffer())
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
