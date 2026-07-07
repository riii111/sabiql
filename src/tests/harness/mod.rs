pub mod fixtures;

use std::sync::Arc;
use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;

use sabiql_app::model::app_state::AppState;
use sabiql_app::services::AppServices;
use sabiql_infra::adapters::PostgresAdapter;
use sabiql_ui::shell::layout::MainLayout;
use sabiql_ui::theme::{ThemePalette, palette_for};

pub const TEST_WIDTH: u16 = 165;
pub const TEST_HEIGHT: u16 = 51;

pub fn test_instant() -> Instant {
    Instant::now()
}

pub fn create_test_state() -> AppState {
    let mut state = AppState::new("test_project".to_string());
    state.session.active_connection_name = Some("localhost:5432/test".to_string());
    state
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
    let services = render_services();
    terminal
        .draw(|frame| {
            let output = MainLayout::render_with_theme(
                frame,
                state,
                Some(FIXED_TIME_MS),
                &services,
                now,
                theme,
            );
            state.ui.inspector_viewport_plan = output.inspector_viewport_plan;
            state.ui.result_viewport_plan = output.result_viewport_plan;
            state.ui.result_widths_cache = output.result_widths_cache;
            state.ui.inspector_pane_height = output.inspector_pane_height;
            state.ui.result_pane_height = output.result_pane_height;
            if let Some(rows) = output.jsonb_detail_editor_visible_rows {
                state.ui.jsonb_detail_editor_visible_rows = rows;
            }
            if let Some(rows) = output.row_detail_content_visible_rows {
                state.ui.row_detail_content_visible_rows = rows;
            }
            if let Some(columns) = output.row_detail_content_visible_columns {
                state.ui.row_detail_content_visible_columns = columns;
            }
            if output.row_detail_content_visible_rows.is_some()
                || output.row_detail_content_visible_columns.is_some()
            {
                state.row_detail.clamp_scroll(
                    state.ui.row_detail_content_visible_rows,
                    state.ui.row_detail_content_visible_columns,
                );
            }
        })
        .unwrap();

    terminal.backend().buffer().clone()
}

fn render_services() -> AppServices {
    AppServices {
        dsn_builder: Arc::new(PostgresAdapter::new()),
        ..AppServices::stub()
    }
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

pub fn connected_state() -> AppState {
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata()));
    state
}

pub fn explorer_selected_state() -> AppState {
    let mut state = connected_state();
    state.ui.set_explorer_selection(Some(0));
    state
}

pub fn table_detail_loaded_state() -> AppState {
    let mut state = explorer_selected_state();
    let _ = state
        .session
        .set_table_detail(fixtures::sample_table_detail(), 0);
    state
}

pub fn with_current_result(state: &mut AppState) {
    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result()));
}
