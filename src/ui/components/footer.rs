use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::status_message::{MessageType, StatusMessage};
use crate::app::input_mode::InputMode;
use crate::app::mode::Mode;
use crate::app::state::AppState;

pub struct Footer;

impl Footer {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        if let Some(error) = &state.last_error {
            let line = StatusMessage::render_line(error, MessageType::Error);
            frame.render_widget(Paragraph::new(line), area);
        } else if let Some(success) = &state.last_success {
            let line = StatusMessage::render_line(success, MessageType::Success);
            frame.render_widget(Paragraph::new(line), area);
        } else {
            let hints = Self::get_context_hints(state);
            let line = Self::build_hint_line(&hints);
            frame.render_widget(Paragraph::new(line), area);
        }
    }

    fn get_context_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
        use crate::app::focused_pane::FocusedPane;

        match state.input_mode {
            InputMode::Normal => {
                if state.focus_mode {
                    vec![
                        ("f", "Exit Focus"),
                        ("j/k", "Scroll"),
                        ("h/l", "H-Scroll"),
                        ("g/G", "Top/Bottom"),
                        ("1/2/3", "Pane"),
                        ("?", "Help"),
                        ("q", "Quit"),
                    ]
                } else if state.mode == Mode::ER {
                    vec![
                        ("q", "Quit"),
                        ("?", "Help"),
                        ("1/2", "Pane"),
                        ("j/k", "Navigate"),
                        ("⏎", "Recenter"),
                        ("d", "Depth"),
                        (":erd", "Export"),
                    ]
                } else {
                    let mut hints = vec![("q", "Quit"), ("?", "Help"), ("1/2/3", "Pane")];
                    hints.push(("f", "Focus"));
                    // Show scroll hint when Result pane is focused
                    if state.focused_pane == FocusedPane::Result {
                        hints.push(("j/k/g/G", "Scroll"));
                        hints.push(("h/l", "H-Scroll"));
                    }
                    hints.push(("[/]", "InsTabs"));
                    hints.push(("r", "Reload"));
                    hints.push(("c", "Console"));
                    hints.push(("s", "SQL"));
                    hints.push(("^P", "Tables"));
                    hints.push(("^K", "Palette"));
                    hints
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
            InputMode::SqlModal => vec![("⌥Enter", "Run"), ("Esc", "Close"), ("↑↓←→", "Move")],
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
