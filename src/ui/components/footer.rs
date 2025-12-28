use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::input_mode::InputMode;
use crate::app::mode::Mode;
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
            InputMode::Normal => {
                if state.focus_mode {
                    // Focus mode: minimal hints
                    vec![
                        ("f", "Exit Focus"),
                        ("j/k", "Scroll"),
                        ("?", "Help"),
                        ("q", "Quit"),
                    ]
                } else {
                    let pane_hint = match state.mode {
                        Mode::Browse => ("1/2/3", "Pane"),
                        Mode::ER => ("1/2", "Pane"),
                    };
                    vec![
                        ("q", "Quit"),
                        ("^P", "Tables"),
                        (":", "Cmd"),
                        ("?", "Help"),
                        ("f", "Focus"),
                        pane_hint,
                        ("[/]", "InsTabs"),
                        ("r", "Reload"),
                    ]
                }
            }
            InputMode::CommandLine => vec![("Enter", "Execute"), ("Esc", "Cancel")],
            InputMode::TablePicker => vec![
                ("Esc", "Close"),
                ("Enter", "Select"),
                ("↑↓", "Navigate"),
                ("type", "Filter"),
            ],
            InputMode::CommandPalette => {
                vec![("Esc", "Close"), ("Enter", "Execute"), ("↑↓", "Navigate")]
            }
            InputMode::Help => vec![("q", "Quit"), ("?/Esc", "Close")],
            InputMode::SqlModal => vec![
                ("^Enter", "Run"),
                ("Esc", "Close"),
                ("↑↓←→", "Move"),
            ],
        }
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
