use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::ui::primitives::atoms::{key_chip, key_text};
use crate::ui::theme::Theme;

pub fn hint_line(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();

    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(key_text(key));
        spans.push(Span::raw(format!(":{}", desc)));
    }

    Line::from(spans)
}

pub fn chip_hint_line(key: &str, desc: &str) -> Line<'static> {
    let chip = key_chip(key);
    let padding_len = 15usize.saturating_sub(key.len() + 4);

    Line::from(vec![
        Span::raw("  "),
        chip,
        Span::raw(" ".repeat(padding_len)),
        Span::styled(desc.to_string(), Style::default().fg(Theme::TEXT_SECONDARY)),
    ])
}
