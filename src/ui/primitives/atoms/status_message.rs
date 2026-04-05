use ratatui::text::{Line, Span};

use crate::ui::theme::{StatusTone, ThemePalette};

pub enum MessageType {
    Error,
    Success,
}

pub struct StatusMessage;

impl StatusMessage {
    pub fn render_line(
        message: &str,
        msg_type: MessageType,
        theme: &ThemePalette,
    ) -> Line<'static> {
        let (prefix, style) = match msg_type {
            MessageType::Error => ("", theme.status_style(StatusTone::Error)),
            MessageType::Success => ("", theme.status_style(StatusTone::Success)),
        };

        Line::from(vec![Span::styled(format!("{prefix}{message}"), style)])
    }
}
