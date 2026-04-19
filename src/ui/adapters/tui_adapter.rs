use std::time::Instant;

use crossterm::cursor::SetCursorStyle;
use crossterm::execute;

use crate::app::ui::model::app_state::AppState;
use crate::app::ui::model::shared::input_mode::InputMode;
use crate::app::ui::ports::renderer::{RenderOutput, RenderResult, Renderer};
use crate::app::services::AppServices;
use crate::ui::shell::layout::MainLayout;
use crate::ui::tui::TuiRunner;

pub struct TuiAdapter<'a> {
    tui: &'a mut TuiRunner,
    last_cursor_insert: Option<bool>,
}

impl<'a> TuiAdapter<'a> {
    pub fn new(tui: &'a mut TuiRunner) -> Self {
        Self {
            tui,
            last_cursor_insert: None,
        }
    }
}

impl Renderer for TuiAdapter<'_> {
    fn draw(
        &mut self,
        state: &AppState,
        services: &AppServices,
        now: Instant,
    ) -> RenderResult<RenderOutput> {
        let mut output = RenderOutput::default();
        let time_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        self.tui.terminal().draw(|frame| {
            output = MainLayout::render(frame, state, time_ms, services, now);
        })?;
        let uses_insert = uses_insert_cursor(state);
        if self.last_cursor_insert != Some(uses_insert) {
            execute!(
                std::io::stdout(),
                if uses_insert {
                    SetCursorStyle::SteadyBar
                } else {
                    SetCursorStyle::SteadyBlock
                }
            )?;
            self.last_cursor_insert = Some(uses_insert);
        }
        Ok(output)
    }
}

fn uses_insert_cursor(state: &AppState) -> bool {
    match state.input_mode() {
        InputMode::JsonbEdit => true,
        InputMode::JsonbDetail => state.jsonb_detail.search().active,
        InputMode::SqlModal => matches!(
            state.sql_modal.status(),
            crate::app::ui::model::sql_editor::modal::SqlModalStatus::Editing
        ),
        _ => false,
    }
}
