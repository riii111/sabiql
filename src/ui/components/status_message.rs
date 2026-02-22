use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::ui::theme::Theme;

pub enum MessageType {
    Error,
    Success,
}

pub struct StatusMessage;

impl StatusMessage {
    pub fn render_line(message: &str, msg_type: MessageType) -> Line<'static> {
        let (prefix, color) = match msg_type {
            MessageType::Error => ("", Theme::STATUS_ERROR),
            MessageType::Success => ("", Theme::STATUS_SUCCESS),
        };

        Line::from(vec![Span::styled(
            format!("{}{}", prefix, message),
            Style::default().fg(color),
        )])
    }
}
