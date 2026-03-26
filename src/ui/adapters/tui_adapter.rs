use std::time::Instant;

use color_eyre::eyre::Result;

use crate::app::model::app_state::AppState;
use crate::app::ports::renderer::{RenderOutput, Renderer};
use crate::app::services::AppServices;
use crate::ui::shell::layout::MainLayout;
use crate::ui::tui::TuiRunner;

pub struct TuiAdapter<'a> {
    tui: &'a mut TuiRunner,
}

impl<'a> TuiAdapter<'a> {
    pub fn new(tui: &'a mut TuiRunner) -> Self {
        Self { tui }
    }
}

impl Renderer for TuiAdapter<'_> {
    fn draw(
        &mut self,
        state: &mut AppState,
        services: &AppServices,
        now: Instant,
    ) -> Result<RenderOutput> {
        let mut output = RenderOutput::default();
        self.tui.terminal().draw(|frame| {
            output = MainLayout::render(frame, state, None, services, now);
        })?;
        Ok(output)
    }
}
