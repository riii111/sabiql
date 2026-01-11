use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::status_message::{MessageType, StatusMessage};
use crate::app::er_state::ErStatus;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

pub struct Footer;

impl Footer {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState, time_ms: Option<u128>) {
        if state.er_preparation.status == ErStatus::Waiting {
            let line = Self::build_er_waiting_line(state, time_ms);
            frame.render_widget(Paragraph::new(line), area);
        } else if let Some(error) = &state.messages.last_error {
            let line = StatusMessage::render_line(error, MessageType::Error);
            frame.render_widget(Paragraph::new(line), area);
        } else if let Some(success) = &state.messages.last_success {
            let line = StatusMessage::render_line(success, MessageType::Success);
            frame.render_widget(Paragraph::new(line), area);
        } else {
            let hints = Self::get_context_hints(state);
            let line = Self::build_hint_line(&hints);
            frame.render_widget(Paragraph::new(line), area);
        }
    }

    fn build_er_waiting_line(state: &AppState, time_ms: Option<u128>) -> Line<'static> {
        let now_ms = time_ms.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        });
        let spinner = spinner_frame(now_ms);

        let total = state.er_preparation.total_tables;
        let failed_count = state.er_preparation.failed_tables.len();
        let remaining =
            state.er_preparation.pending_tables.len() + state.er_preparation.fetching_tables.len();
        let cached = total.saturating_sub(remaining + failed_count);

        let text = format!("{} Preparing ER... ({}/{})", spinner, cached, total);
        Line::from(Span::styled(text, Style::default().fg(Color::Yellow)))
    }

    fn get_context_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
        use crate::app::focused_pane::FocusedPane;

        match state.ui.input_mode {
            InputMode::Normal => {
                if state.ui.focus_mode {
                    vec![
                        ("f", "Exit Focus"),
                        ("j/k", "Scroll"),
                        ("h/l", "H-Scroll"),
                        ("g/G", "Top/Bottom"),
                        ("?", "Help"),
                        ("q", "Quit"),
                    ]
                } else {
                    // Primary actions
                    let mut hints = vec![
                        ("r", "Reload"),
                        ("s", "SQL"),
                        ("e", "ER Diagram"),
                        ("c", "Connect"),
                        ("^P", "Tables"),
                        ("^K", "Palette"),
                    ];
                    // Conditional action
                    if state.connection_error.error_info.is_some() {
                        hints.push(("Enter", "Error"));
                    }
                    // Navigation
                    hints.push(("1/2/3", "Pane"));
                    hints.push(("f", "Focus"));
                    if state.ui.focused_pane == FocusedPane::Result {
                        hints.push(("j/k/g/G", "Scroll"));
                        hints.push(("h/l", "H-Scroll"));
                    }
                    if state.ui.focused_pane == FocusedPane::Inspector {
                        hints.push(("Tab/⇧Tab", "InsTabs"));
                    }
                    // Help and Quit last
                    hints.push(("?", "Help"));
                    hints.push(("q", "Quit"));
                    hints
                }
            }
            InputMode::CommandLine => vec![("Enter", "Execute"), ("Esc", "Cancel")],
            InputMode::TablePicker => vec![
                ("Enter", "Select"),
                ("type", "Filter"),
                ("↑↓", "Navigate"),
                ("Esc", "Close"),
            ],
            InputMode::CommandPalette => {
                vec![("Enter", "Execute"), ("↑↓", "Navigate"), ("Esc", "Close")]
            }
            InputMode::Help => vec![("?/Esc", "Close"), ("q", "Quit")],
            InputMode::SqlModal => vec![("⌥Enter", "Run"), ("↑↓←→", "Move"), ("Esc", "Close")],
            InputMode::ConnectionSetup => vec![
                ("^S", "Save"),
                ("Tab", "Next"),
                ("⇧Tab", "Prev"),
                ("Esc", "Cancel"),
            ],
            InputMode::ConnectionError => vec![
                ("Enter/r", "Retry"),
                ("e", "Edit"),
                ("d", "Details"),
                ("c", "Copy"),
                ("Esc", "Close"),
                ("q", "Quit"),
            ],
            InputMode::ConfirmDialog => vec![("Esc", "Close")],
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

const SPINNER_FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];

fn spinner_frame(time_ms: u128) -> &'static str {
    SPINNER_FRAMES[(time_ms / 300) as usize % SPINNER_FRAMES.len()]
}
