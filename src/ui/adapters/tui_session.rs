use color_eyre::eyre::Result;

use crate::app::ports::tui_session::TuiSession;
use crate::ui::tui::TuiRunner;

pub struct TuiSessionAdapter<'a> {
    tui: &'a mut TuiRunner,
}

impl<'a> TuiSessionAdapter<'a> {
    pub fn new(tui: &'a mut TuiRunner) -> Self {
        Self { tui }
    }
}

impl TuiSession for TuiSessionAdapter<'_> {
    fn suspend(&mut self) -> Result<()> {
        self.tui.suspend()
    }

    fn resume(&mut self) -> Result<()> {
        self.tui.resume()
    }
}
