use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub enum MessageType {
    Error,
    Success,
}

pub struct StatusMessage;

impl StatusMessage {
    pub fn render_line(message: &str, msg_type: MessageType) -> Line<'static> {
        let (prefix, color) = match msg_type {
            MessageType::Error => ("", Color::Red),
            MessageType::Success => ("", Color::Green),
        };

        Line::from(vec![Span::styled(
            format!("{}{}", prefix, message),
            Style::default().fg(color),
        )])
    }
}
