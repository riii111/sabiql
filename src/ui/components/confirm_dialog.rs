use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};

use super::overlay::{centered_rect, render_scrim};
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct ConfirmDialog;

impl ConfirmDialog {
    pub fn render(frame: &mut Frame, _state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        );
        render_scrim(frame);
        frame.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Confirm (placeholder) ")
            .style(Style::default().bg(Theme::MODAL_BG));
        frame.render_widget(block, area);
    }
}
