use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::molecules::render_modal;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct ConfirmDialog;

impl ConfirmDialog {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let dialog = &state.confirm_dialog;

        let message_lines: Vec<&str> = dialog.message.lines().collect();
        let message_height = message_lines.len() as u16;
        let modal_height = message_height + 6;

        let hint = " Enter/Y: Yes │ Esc/N: No ";
        let title = format!(" {} ", dialog.title);
        let (_, modal_inner) = render_modal(
            frame,
            Constraint::Length(45),
            Constraint::Length(modal_height),
            &title,
            hint,
        );

        let inner = modal_inner.inner(Margin::new(1, 0));
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
