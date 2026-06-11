use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::shared::text_input::TextInputState;
use crate::primitives::atoms::text_cursor_spans;
use crate::theme::ThemePalette;

const FILTER_PREFIX: &str = "  > ";

// The block cursor at end-of-content occupies one cell beyond the text,
// so the text viewport must shrink by one to keep the cursor visible.
fn filter_visible_width(raw_width: usize, cursor: usize, char_count: usize) -> usize {
    if cursor == char_count {
        raw_width.saturating_sub(1)
    } else {
        raw_width
    }
}

/// Renders a one-line `  > ` filter input with cursor; returns the visible
/// width the caller must feed back into `TextInputState::update_viewport`.
pub fn render_filter_input_line(
    frame: &mut Frame,
    area: Rect,
    input: &TextInputState,
    placeholder: Option<&str>,
    theme: &ThemePalette,
) -> usize {
    let raw_width = area.width.saturating_sub(FILTER_PREFIX.len() as u16) as usize;
    let visible_width = filter_visible_width(raw_width, input.cursor(), input.char_count());

    let filter_line = match placeholder {
        Some(text) if input.content().is_empty() => Line::from(Span::styled(
            format!("  {text}"),
            Style::default().fg(theme.semantic.text.placeholder),
        )),
        _ => {
            let mut spans = vec![Span::styled(
                FILTER_PREFIX,
                Style::default().fg(theme.component.modal.title),
            )];
            spans.extend(text_cursor_spans(
                input.content(),
                input.cursor(),
                input.viewport_offset(),
                visible_width,
                theme,
            ));
            Line::from(spans)
        }
    };

    frame.render_widget(Paragraph::new(filter_line), area);
    visible_width
}
