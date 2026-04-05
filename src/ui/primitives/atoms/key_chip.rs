use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::ui::theme::ThemePalette;

pub fn key_chip(key: &str, theme: &ThemePalette) -> Span<'static> {
    Span::styled(
        format!(" {key} "),
        Style::default()
            .bg(theme.key_chip_bg)
            .fg(theme.key_chip_fg)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn key_text(key: &str, theme: &ThemePalette) -> Span<'static> {
    Span::styled(key.to_string(), Style::default().fg(theme.text_accent))
}
