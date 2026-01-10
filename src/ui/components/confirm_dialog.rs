use ratatui::prelude::*;
use ratatui::widgets::{Clear, Paragraph};

use super::overlay::{centered_rect, modal_block_with_hint, render_scrim};
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct ConfirmDialog;

impl ConfirmDialog {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let dialog = &state.confirm_dialog;

        let message_lines: Vec<&str> = dialog.message.lines().collect();
        let message_height = message_lines.len() as u16;
        let modal_height = message_height + 6; // title + message + spacer + buttons + borders

        let area = centered_rect(
            frame.area(),
            Constraint::Length(45),
            Constraint::Length(modal_height),
        );
        render_scrim(frame);
        frame.render_widget(Clear, area);

        let hint = " Enter/Y: Yes │ Esc/N: No ";
        let title = format!(" {} ", dialog.title);
        let block = modal_block_with_hint(title, hint.to_string());
        frame.render_widget(block, area);

        let inner = area.inner(Margin::new(2, 1));
        let chunks = Layout::vertical([
            Constraint::Length(message_height),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

        let message_para = Paragraph::new(dialog.message.clone())
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center);
        frame.render_widget(message_para, chunks[0]);

        let buttons = "      [ Yes (Enter) ]   [ No (Esc) ]      ";
        let buttons_para = Paragraph::new(buttons)
            .style(Style::default().fg(Theme::MODAL_HINT))
            .alignment(Alignment::Center);
        frame.render_widget(buttons_para, chunks[2]);
    }
}
