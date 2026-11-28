use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::theme::ThemePalette;

pub(crate) fn key_chip(key: &str, theme: &ThemePalette) -> Span<'static> {
    Span::styled(
        format!(" {key} "),
        Style::default()
            .bg(theme.component.navigation.key_chip_bg)
            .fg(theme.component.navigation.key_chip_fg)
            .add_modifier(Modifier::BOLD),
    )
}

pub(crate) fn key_text(key: &str, theme: &ThemePalette) -> Span<'static> {
    Span::styled(
        key.to_string(),
        Style::default().fg(theme.semantic.text.accent),
    )
}
