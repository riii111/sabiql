use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

pub struct Footer;

impl Footer {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let hints = Self::get_context_hints(state);
        let line = Self::build_hint_line(&hints);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn get_context_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
        match state.input_mode {
            InputMode::CommandLine => {
                return vec![("Enter", "Execute"), ("Esc", "Cancel")];
            }
            InputMode::Filter => {
                return vec![("Enter", "Select"), ("Esc", "Close"), ("↑↓", "Navigate")];
            }
            InputMode::Normal => {}
        }

        if state.show_table_picker {
            return vec![
                ("Esc", "Close"),
                ("Enter", "Select"),
                ("↑↓", "Navigate"),
                ("type", "Filter"),
            ];
        }
        if state.show_command_palette {
            return vec![("Esc", "Close"), ("Enter", "Execute"), ("↑↓", "Navigate")];
        }
        if state.show_help {
            return vec![("?/Esc", "Close"), ("↑↓", "Scroll")];
        }

        vec![
            ("q", "Quit"),
            ("^P", "Tables"),
            ("^K", "Cmds"),
            (":", "Cmd"),
            ("?", "Help"),
            ("f", "Focus"),
            ("1/2", "Tab"),
        ]
    }

    fn build_hint_line(hints: &[(&str, &str)]) -> Line<'static> {
        let mut spans = Vec::new();
        for (i, (key, desc)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                (*key).to_string(),
                Style::default().fg(Color::Yellow),
            ));
            spans.push(Span::raw(format!(":{}", desc)));
        }
        Line::from(spans)
    }
}
