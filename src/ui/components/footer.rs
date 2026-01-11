use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::atoms::spinner_char;
use super::status_message::{MessageType, StatusMessage};
use crate::app::er_state::ErStatus;
use crate::app::input_mode::InputMode;
use crate::app::keybindings::footer as hints;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct Footer;

impl Footer {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState, time_ms: Option<u128>) {
        if state.er_preparation.status == ErStatus::Waiting {
            let line = Self::build_er_waiting_line(state, time_ms);
            frame.render_widget(Paragraph::new(line), area);
        } else if let Some(error) = &state.messages.last_error {
            let line = StatusMessage::render_line(error, MessageType::Error);
            frame.render_widget(Paragraph::new(line), area);
        } else {
            // Show hints with optional inline success message
            let hints = Self::get_context_hints(state);
            let line =
                Self::build_hint_line_with_success(&hints, state.messages.last_success.as_deref());
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
        let spinner = spinner_char(now_ms);

        let total = state.er_preparation.total_tables;
        let failed_count = state.er_preparation.failed_tables.len();
        let remaining =
            state.er_preparation.pending_tables.len() + state.er_preparation.fetching_tables.len();
        let cached = total.saturating_sub(remaining + failed_count);

        let text = format!("{} Preparing ER... ({}/{})", spinner, cached, total);
        Line::from(Span::styled(text, Style::default().fg(Color::Yellow)))
    }

    /// Hint ordering: Actions → Navigation → Help → Close/Cancel → Quit
    fn get_context_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
        use crate::app::focused_pane::FocusedPane;

        match state.ui.input_mode {
            InputMode::Normal => {
                if state.ui.focus_mode {
                    vec![
                        hints::EXIT_FOCUS,
                        hints::SCROLL_SHORT,
                        hints::H_SCROLL,
                        hints::TOP_BOTTOM,
                        hints::HELP,
                        hints::QUIT,
                    ]
                } else {
                    let mut list = vec![
                        hints::RELOAD,
                        hints::SQL,
                        hints::ER_DIAGRAM,
                        hints::CONNECT,
                        hints::TABLE_PICKER,
                        hints::PALETTE,
                    ];
                    if state.connection_error.error_info.is_some() {
                        list.push(hints::ERROR_OPEN);
                    }
                    list.push(hints::PANE_SWITCH);
                    list.push(hints::FOCUS);
                    if state.ui.focused_pane == FocusedPane::Result {
                        list.push(hints::SCROLL);
                        list.push(hints::H_SCROLL);
                    }
                    if state.ui.focused_pane == FocusedPane::Inspector {
                        list.push(hints::INSPECTOR_TABS);
                    }
                    list.push(hints::HELP);
                    list.push(hints::QUIT);
                    list
                }
            }
            InputMode::CommandLine => vec![hints::ENTER_EXECUTE, hints::ESC_CANCEL],
            InputMode::TablePicker => vec![
                hints::ENTER_SELECT,
                hints::TYPE_FILTER,
                hints::NAVIGATE,
                hints::ESC_CLOSE,
            ],
            InputMode::CommandPalette => {
                vec![hints::ENTER_EXECUTE, hints::NAVIGATE, hints::ESC_CLOSE]
            }
            InputMode::Help => vec![hints::HELP_SCROLL, hints::HELP_CLOSE, hints::QUIT],
            InputMode::SqlModal => vec![hints::SQL_RUN, hints::SQL_MOVE, hints::ESC_CLOSE],
            InputMode::ConnectionSetup => vec![
                hints::SAVE,
                hints::TAB_NEXT,
                hints::TAB_PREV,
                hints::ESC_CANCEL,
            ],
            InputMode::ConnectionError => vec![
                hints::EDIT,
                hints::DETAILS,
                hints::COPY,
                hints::ESC_CLOSE,
                hints::QUIT,
            ],
            InputMode::ConfirmDialog => vec![hints::ESC_CLOSE],
        }
    }

    fn build_hint_line_with_success(
        hints: &[(&str, &str)],
        success_msg: Option<&str>,
    ) -> Line<'static> {
        let mut spans = Vec::new();

        if let Some(msg) = success_msg {
            spans.push(Span::styled(
                format!("✓ {}  ", msg),
                Style::default().fg(Color::Green),
            ));
        }

        for (i, (key, desc)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                (*key).to_string(),
                Style::default().fg(Theme::TEXT_ACCENT),
            ));
            spans.push(Span::raw(format!(":{}", desc)));
        }

        Line::from(spans)
    }
}
