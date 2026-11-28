use crate::app::model::shared::text_input::input_viewport_offset;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::shared::text_input::TextInputState;
use crate::primitives::atoms::text_cursor_spans;
use crate::theme::ThemePalette;

const FILTER_PREFIX: &str = "  > ";

/// Renders a one-line `  > ` filter input with cursor; returns the visible
/// width the caller must feed back into `TextInputState::update_viewport`.
pub(crate) fn render_filter_input_line(
    frame: &mut Frame,
    area: Rect,
    input: &TextInputState,
    placeholder: Option<&str>,
    theme: &ThemePalette,
) -> usize {
    let raw_width = area.width.saturating_sub(FILTER_PREFIX.len() as u16) as usize;

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
                input_viewport_offset(
                    input.content(),
                    input.cursor(),
                    input.viewport_offset(),
                    raw_width,
                    true,
                ),
                raw_width,
                theme,
            ));
            Line::from(spans)
        }
    };

    frame.render_widget(Paragraph::new(filter_line), area);
    raw_width
}
