use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::ui::primitives::atoms::{key_chip, key_text};
use crate::ui::theme::ThemePalette;

pub fn hint_line(hints: &[(&str, &str)], theme: &ThemePalette) -> Line<'static> {
    let mut spans = Vec::new();

    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(key_text(key, theme));
        spans.push(Span::raw(format!(":{desc}")));
    }

    Line::from(spans)
}

pub fn chip_hint_line(key: &str, desc: &str, theme: &ThemePalette) -> Line<'static> {
    let chip = key_chip(key, theme);
    let padding_len = 15usize.saturating_sub(key.len() + 4);

    Line::from(vec![
        Span::raw("  "),
        chip,
        Span::raw(" ".repeat(padding_len)),
        Span::styled(desc.to_string(), Style::default().fg(theme.text_secondary)),
    ])
}
