use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::ui::primitives::atoms::text_cursor_spans;
use crate::ui::theme::ThemePalette;

pub struct CommandLine;

impl CommandLine {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) -> usize {
        let raw_width = area.width.saturating_sub(1) as usize; // ":" prefix

        let content = if state.input_mode() == InputMode::CommandLine {
            let input = &state.command_line_input;
            let visible_width = if input.cursor() == input.char_count() {
                raw_width.saturating_sub(1)
            } else {
                raw_width
            };
            let cursor_spans = text_cursor_spans(
                input.content(),
                input.cursor(),
                input.viewport_offset(),
                visible_width,
                theme,
            );
            let mut spans = vec![Span::styled(":", Style::default().fg(theme.text_accent))];
            spans.extend(cursor_spans);
            Line::from(spans)
        } else {
            Line::raw("")
        };

        frame.render_widget(Paragraph::new(content), area);
        raw_width
    }
}
