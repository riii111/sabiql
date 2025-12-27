use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::state::AppState;

pub struct Explorer;

impl Explorer {
    pub fn render(frame: &mut Frame, area: Rect, _state: &AppState) {
        let block = Block::default().title("Explorer").borders(Borders::ALL);
        let content = Paragraph::new("(tables will be listed here)").block(block);
        frame.render_widget(content, area);
    }
}
