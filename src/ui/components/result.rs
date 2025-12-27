use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::state::AppState;

pub struct ResultPane;

impl ResultPane {
    pub fn render(frame: &mut Frame, area: Rect, _state: &AppState) {
        let block = Block::default().title("Result").borders(Borders::ALL);
        let content = Paragraph::new("(preview will appear here)").block(block);
        frame.render_widget(content, area);
    }
}
