use color_eyre::eyre::Result;

use crate::app::ports::renderer::{RenderOutput, Renderer};
use crate::app::state::AppState;
use crate::ui::components::layout::MainLayout;
use crate::ui::tui::TuiRunner;

pub struct TuiRenderer<'a> {
    tui: &'a mut TuiRunner,
}

impl<'a> TuiRenderer<'a> {
    pub fn new(tui: &'a mut TuiRunner) -> Self {
        Self { tui }
    }
}

impl Renderer for TuiRenderer<'_> {
    fn draw(&mut self, state: &mut AppState) -> Result<RenderOutput> {
        let mut output = RenderOutput::default();
        self.tui.terminal().draw(|frame| {
            output = MainLayout::render(frame, state, None);
        })?;
        Ok(output)
    }
}
