use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use super::molecules::render_modal;
use crate::app::state::AppState;

pub struct ConfirmDialog;

impl ConfirmDialog {
    fn wrapped_line_count(text: &str, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }

        text.lines()
            .map(|line| {
                let chars = line.chars().count() as u16;
                chars.max(1).div_ceil(width)
            })
            .sum()
    }

    pub fn render(frame: &mut Frame, state: &AppState) {
        let dialog = &state.confirm_dialog;
        let hint = " Enter/Y: Confirm │ Esc/N: Cancel ";

        let full_area = frame.area();
        let max_modal_width = full_area.width.saturating_sub(2).max(20);
        let message_max_line = dialog
            .message
            .lines()
            .map(|line| line.chars().count() as u16)
            .max()
            .unwrap_or(0);
        let hint_width = hint.chars().count() as u16;
        let title_width = dialog.title.chars().count() as u16;
        let content_width = message_max_line.max(hint_width).max(title_width);
        let preferred_width = content_width.saturating_add(6).max(45);
        let modal_width = preferred_width.min(max_modal_width);

        let message_width = modal_width.saturating_sub(4).max(1);
        let message_height = Self::wrapped_line_count(&dialog.message, message_width);
        let max_modal_height = full_area.height.saturating_sub(2).max(8);
        let modal_height = (message_height + 4).min(max_modal_height);

        let title = format!(" {} ", dialog.title);
        let (_, modal_inner) = render_modal(
            frame,
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
            &title,
            hint,
        );

        let inner = modal_inner.inner(Margin::new(1, 0));
        let message_para = Paragraph::new(dialog.message.clone())
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        frame.render_widget(message_para, inner);
    }
}
