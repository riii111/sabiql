use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::{StatusTone, ThemePalette};

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

    pub fn render_lines(
        message: &str,
        msg_type: MessageType,
        width: u16,
        theme: &ThemePalette,
    ) -> Vec<Line<'static>> {
        if width == 0 {
            return Vec::new();
        }

        let style = match msg_type {
            MessageType::Error => theme.status_style(StatusTone::Error),
            MessageType::Success => theme.status_style(StatusTone::Success),
        };
        let width = width as usize;
        let mut lines = Vec::new();

        for source_line in message.split('\n') {
            for line in wrap_line(source_line, width) {
                lines.push(Line::from(Span::styled(line, style)));
            }
        }

        lines
    }
}

fn wrap_line(source: &str, width: usize) -> Vec<String> {
    if UnicodeWidthStr::width(source) <= width {
        return vec![source.to_string()];
    }

    let mut lines = Vec::new();
    let mut remaining = source;
    while UnicodeWidthStr::width(remaining) > width {
        let mut cut = 0;
        let mut used = 0;
        for (index, ch) in remaining.char_indices() {
            let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + char_width > width {
                break;
            }
            used += char_width;
            cut = index + ch.len_utf8();
        }

        if cut == 0 {
            cut = remaining
                .char_indices()
                .nth(1)
                .map_or(remaining.len(), |(index, _)| index);
        }

        let candidate = &remaining[..cut];
        let split_at = candidate
            .char_indices()
            .rev()
            .find_map(|(index, ch)| ch.is_whitespace().then_some(index));
        let (line, next) = split_at.filter(|index| *index > 0).map_or_else(
            || (candidate, &remaining[cut..]),
            |index| (&candidate[..index], &remaining[index..]),
        );
        lines.push(line.trim_end().to_string());
        remaining = next.trim_start();
    }
    lines.push(remaining.to_string());
    lines
}
