pub mod fixtures;
pub mod postgres;

use std::sync::Arc;
use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;

use sabiql_app::model::app_state::AppState;
use sabiql_app::model::connection::setup::ConnectionField;
use sabiql_app::model::shared::text_input::TextInputState;
use sabiql_app::update::action::{Action, ModalKind};
use sabiql_app::update::reducer::reduce;
use sabiql_domain::ConnectionId;
use sabiql_domain::test_support::endpoint;
use sabiql_ui::shell::layout::MainLayout;
use sabiql_ui::theme::{ThemePalette, palette_for};

pub const TEST_WIDTH: u16 = 165;
pub const TEST_HEIGHT: u16 = 51;

pub fn test_instant() -> Instant {
    Instant::now()
}

pub fn open_command_palette(state: &mut AppState) {
    reduce(
        state,
        Action::OpenModal(ModalKind::CommandPalette),
        test_instant(),
    );
}

pub fn create_test_state() -> AppState {
    let mut state = AppState::new("test_project".to_string());
    state.session.activate_connection(
        &ConnectionId::from_string("test-connection"),
        "localhost:5432/test",
        &endpoint::postgres("localhost:5432/test"),
    );
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
    render_and_get_buffer_at_with_theme(terminal, state, now, palette_for(state.ui.theme_id))
}

pub fn render_and_get_buffer_at_with_theme(
    terminal: &mut Terminal<TestBackend>,
    state: &mut AppState,
    now: Instant,
    theme: &ThemePalette,
) -> Buffer {
    terminal
        .draw(|frame| {
            let output =
                MainLayout::render_with_theme(frame, state, Some(FIXED_TIME_MS), now, theme);
            state.apply_render_output(output);
        })
        .unwrap();

    terminal.backend().buffer().clone()
}

pub fn render_to_string(terminal: &mut Terminal<TestBackend>, state: &mut AppState) -> String {
    let buffer = render_and_get_buffer(terminal, state);
    area_to_string(&buffer, buffer.area)
}

pub fn render_and_get_cursor_position(
    terminal: &mut Terminal<TestBackend>,
    state: &mut AppState,
) -> Position {
    let _ = render_and_get_buffer(terminal, state);
    terminal.backend_mut().get_cursor_position().unwrap()
}

pub fn render_modal_to_string(
    terminal: &mut Terminal<TestBackend>,
    state: &mut AppState,
) -> String {
    let buffer = render_and_get_buffer(terminal, state);
    modal_contents(&buffer)
}

pub fn modal_contents(buffer: &Buffer) -> String {
    // The scrim dims the background, so the undimmed cells bound the visible modal.
    let area = buffer
        .area
        .positions()
        .filter(|&position| !buffer[position].modifier.contains(Modifier::DIM))
        .map(|position| Rect::new(position.x, position.y, 1, 1))
        .reduce(Rect::union)
        .expect("expected a visible modal");
    area_to_string(buffer, area)
}

pub fn render_footer_to_string(
    terminal: &mut Terminal<TestBackend>,
    state: &mut AppState,
) -> String {
    let buffer = render_and_get_buffer(terminal, state);
    let footer = buffer
        .area
        .rows()
        .nth_back(1)
        .expect("footer above command line");
    area_to_string(&buffer, footer).trim_end().to_string()
}

fn area_to_string(buffer: &Buffer, area: Rect) -> String {
    area.rows()
        .map(|row| {
            row.positions()
                .map(|position| buffer[position].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn postgres_connected_state() -> AppState {
    let mut state = create_test_state();
    state
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata()));
    state
}

pub fn explorer_selected_state() -> AppState {
    let mut state = postgres_connected_state();
    state.ui.set_explorer_selection(Some(0));
    state
}

pub fn table_detail_loaded_state() -> AppState {
    let mut state = explorer_selected_state();
    let _ = state
        .session
        .set_table_detail(fixtures::sample_postgres_table_detail(), 0);
    state
}

pub fn with_current_result(state: &mut AppState) {
    state
        .query
        .set_current_result(Arc::new(fixtures::sample_query_result()));
}
