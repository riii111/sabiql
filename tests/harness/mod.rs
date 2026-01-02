pub mod fixtures;

use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use dbtui::app::state::AppState;
use dbtui::ui::components::layout::MainLayout;

pub const TEST_WIDTH: u16 = 80;
pub const TEST_HEIGHT: u16 = 24;

pub fn test_instant() -> Instant {
    Instant::now()
}

pub fn create_test_state() -> AppState {
    AppState::new("test_project".to_string(), "default".to_string())
}

pub fn create_test_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(TEST_WIDTH, TEST_HEIGHT);
    Terminal::new(backend).unwrap()
}

const FIXED_TIME_MS: u128 = 0;

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
