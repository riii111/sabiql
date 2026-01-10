use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};

use super::overlay::{centered_rect, render_scrim};
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct ConnectionError;

impl ConnectionError {
    pub fn render(frame: &mut Frame, _state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        );
        render_scrim(frame);
        frame.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Connection Error (placeholder) ")
            .style(Style::default().bg(Theme::MODAL_BG));
        frame.render_widget(block, area);
    }
}
