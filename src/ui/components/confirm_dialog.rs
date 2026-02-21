use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use super::molecules::render_modal;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

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

        let full_area = frame.area();
        let max_modal_width = full_area.width.saturating_sub(2).max(20);
        let preferred_width = (full_area.width * 80 / 100).max(45);
        let modal_width = preferred_width.min(max_modal_width);

        let message_width = modal_width.saturating_sub(4).max(1);
        let message_height = Self::wrapped_line_count(&dialog.message, message_width);
        let max_modal_height = full_area.height.saturating_sub(2).max(8);
        let modal_height = (message_height + 6).min(max_modal_height);

        let title = format!(" {} ", dialog.title);
        let (_, modal_inner) = render_modal(
            frame,
            Constraint::Length(modal_width),
            Constraint::Length(modal_height),
            &title,
            "",
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
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        frame.render_widget(message_para, chunks[0]);

        let buttons = "    [ Confirm (Enter) ]   [ Cancel (Esc) ]    ";
        let buttons_para = Paragraph::new(buttons)
            .style(Style::default().fg(Theme::MODAL_HINT))
            .alignment(Alignment::Center);
        frame.render_widget(buttons_para, chunks[2]);
    }
}
