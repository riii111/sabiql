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
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        if state.er_preparation.status == ErStatus::Waiting {
            let line = Self::build_er_waiting_line(state);
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

    fn build_er_waiting_line(state: &AppState) -> Line<'static> {
        const SPINNER_FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];

        // Use system time for spinner animation (wraps every ~1.2 seconds)
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let frame_idx = (now_ms / 300) as usize % SPINNER_FRAMES.len();
        let spinner = SPINNER_FRAMES[frame_idx];

        // Calculate progress from cached total (stable during ER preparation)
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
                        ("1/2/3", "Pane"),
                        ("?", "Help"),
                        ("q", "Quit"),
                    ]
                } else {
                    let mut hints = vec![("q", "Quit"), ("?", "Help"), ("1/2/3", "Pane")];
                    hints.push(("f", "Focus"));
                    // Show scroll hint when Result pane is focused
                    if state.ui.focused_pane == FocusedPane::Result {
                        hints.push(("j/k/g/G", "Scroll"));
                        hints.push(("h/l", "H-Scroll"));
                    }
                    // Show Inspector tab switching hint when Inspector is focused
                    if state.ui.focused_pane == FocusedPane::Inspector {
                        hints.push(("Tab/⇧Tab", "InsTabs"));
                    }
                    hints.push(("r", "Reload"));
                    hints.push(("c", "Console"));
                    hints.push(("s", "SQL"));
                    hints.push(("e", "ER Diagram"));
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
