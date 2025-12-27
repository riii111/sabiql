use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::state::AppState;

pub struct Inspector;

impl Inspector {
    pub fn render(frame: &mut Frame, area: Rect, _state: &AppState) {
        let block = Block::default()
            .title("Inspector [Cols] [Idx] [FK] [RLS] [DDL]")
            .borders(Borders::ALL);
        let content = Paragraph::new("(select a table)").block(block);
        frame.render_widget(content, area);
    }
}
