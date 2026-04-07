use std::time::Instant;

use color_eyre::eyre::Result;
use crossterm::cursor::SetCursorStyle;
use crossterm::execute;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::ports::renderer::{RenderOutput, Renderer};
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
    ) -> Result<RenderOutput> {
        let mut output = RenderOutput::default();
        self.tui.terminal().draw(|frame| {
            output = MainLayout::render(frame, state, None, services, now);
        })?;
        let uses_insert = uses_insert_cursor(state);
        if self.last_cursor_insert != Some(uses_insert) {
            let _ = execute!(
                std::io::stdout(),
                if uses_insert {
                    SetCursorStyle::SteadyBar
                } else {
                    SetCursorStyle::SteadyBlock
                }
            );
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
            crate::app::model::sql_editor::modal::SqlModalStatus::Editing
        ),
        _ => false,
    }
}
