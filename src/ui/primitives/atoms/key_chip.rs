use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::ui::theme::Theme;

pub fn key_chip(key: &str) -> Span<'static> {
    Span::styled(
        format!(" {} ", key),
        Style::default()
            .bg(Theme::KEY_CHIP_BG)
            .fg(Theme::KEY_CHIP_FG)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn key_text(key: &str) -> Span<'static> {
    Span::styled(key.to_string(), Style::default().fg(Theme::TEXT_ACCENT))
}
