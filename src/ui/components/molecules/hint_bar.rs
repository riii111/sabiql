use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::ui::components::atoms::{key_chip, key_text};
use crate::ui::theme::Theme;

/// Creates a hint line for footer display.
/// Format: "key1 desc1  key2 desc2  ..."
///
/// # Arguments
/// * `hints` - Slice of (key, description) tuples
pub fn hint_line(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();

    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(key_text(key));
        spans.push(Span::styled(
            format!(" {}", desc),
            Style::default().fg(Theme::TEXT_SECONDARY),
        ));
    }

    Line::from(spans)
}

/// Creates a chip-style hint line for help overlay.
/// Format: "  [key]  description"
///
/// # Arguments
/// * `key` - Key binding (will be displayed as a chip)
/// * `desc` - Description text
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
