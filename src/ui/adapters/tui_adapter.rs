use color_eyre::eyre::Result;

use crate::app::ports::renderer::{RenderOutput, Renderer};
use crate::app::ports::tui_session::TuiSession;
use crate::app::state::AppState;
use crate::ui::components::layout::MainLayout;
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
    fn draw(&mut self, state: &mut AppState) -> Result<RenderOutput> {
        let mut output = RenderOutput::default();
        self.tui.terminal().draw(|frame| {
            output = MainLayout::render(frame, state, None);
        })?;
        Ok(output)
    }
}

impl TuiSession for TuiAdapter<'_> {
    fn suspend(&mut self) -> Result<()> {
        self.tui.suspend()
    }

    fn resume(&mut self) -> Result<()> {
        self.tui.resume()
    }
}
