use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

pub struct CommandLine;

impl CommandLine {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let content = if state.ui.input_mode == InputMode::CommandLine {
            Line::from(vec![
                Span::styled(":", Style::default().fg(Color::Yellow)),
                Span::raw(&state.command_line_input),
                Span::styled(
                    "█",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ])
        } else {
            Line::raw("")
        };

        frame.render_widget(Paragraph::new(content), area);
    }
}
