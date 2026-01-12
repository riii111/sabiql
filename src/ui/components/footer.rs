use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::atoms::spinner_char;
use super::status_message::{MessageType, StatusMessage};
use crate::app::er_state::ErStatus;
use crate::app::explorer_mode::ExplorerMode;
use crate::app::input_mode::InputMode;
use crate::app::keybindings::{
    COMMAND_PALETTE_KEYS, CONFIRM_DIALOG_KEYS, CONNECTION_ERROR_KEYS, CONNECTION_SETUP_KEYS,
    FOOTER_NAV_KEYS, GLOBAL_KEYS, HELP_KEYS, OVERLAY_KEYS, SQL_MODAL_KEYS, TABLE_PICKER_KEYS, idx,
};
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
                        GLOBAL_KEYS[idx::global::EXIT_FOCUS].as_hint(),
                        FOOTER_NAV_KEYS[idx::footer_nav::SCROLL_SHORT].as_hint(),
                        FOOTER_NAV_KEYS[idx::footer_nav::H_SCROLL].as_hint(),
                        FOOTER_NAV_KEYS[idx::footer_nav::TOP_BOTTOM].as_hint(),
                        GLOBAL_KEYS[idx::global::HELP].as_hint(),
                        GLOBAL_KEYS[idx::global::QUIT].as_hint(),
                    ]
                } else if state.ui.explorer_mode == ExplorerMode::Connections
                    && state.ui.focused_pane == FocusedPane::Explorer
                {
                    // Connections mode hints
                    vec![
                        ("Enter", "Connect"),
                        ("n", "New"),
                        ("j/k", "Navigate"),
                        ("?", "Help"),
                        ("c", "Tables"),
                        ("Esc", "Back"),
                        ("q", "Quit"),
                    ]
                } else {
                    let mut list = vec![
                        GLOBAL_KEYS[idx::global::RELOAD].as_hint(),
                        GLOBAL_KEYS[idx::global::SQL].as_hint(),
                        GLOBAL_KEYS[idx::global::ER_DIAGRAM].as_hint(),
                        GLOBAL_KEYS[idx::global::CONNECTIONS].as_hint(),
                        GLOBAL_KEYS[idx::global::TABLE_PICKER].as_hint(),
                        GLOBAL_KEYS[idx::global::PALETTE].as_hint(),
                    ];
                    if state.connection_error.error_info.is_some() {
                        list.push(OVERLAY_KEYS[idx::overlay::ERROR_OPEN].as_hint());
                    }
                    list.push(GLOBAL_KEYS[idx::global::PANE_SWITCH].as_hint());
                    list.push(GLOBAL_KEYS[idx::global::FOCUS].as_hint());
                    if state.ui.focused_pane == FocusedPane::Result {
                        list.push(FOOTER_NAV_KEYS[idx::footer_nav::SCROLL].as_hint());
                        list.push(FOOTER_NAV_KEYS[idx::footer_nav::H_SCROLL].as_hint());
                    }
                    if state.ui.focused_pane == FocusedPane::Inspector {
                        list.push(GLOBAL_KEYS[idx::global::INSPECTOR_TABS].as_hint());
                    }
                    list.push(GLOBAL_KEYS[idx::global::HELP].as_hint());
                    list.push(GLOBAL_KEYS[idx::global::QUIT].as_hint());
                    list
                }
            }
            InputMode::CommandLine => vec![
                OVERLAY_KEYS[idx::overlay::ENTER_EXECUTE].as_hint(),
                OVERLAY_KEYS[idx::overlay::ESC_CANCEL].as_hint(),
            ],
            InputMode::TablePicker => vec![
                TABLE_PICKER_KEYS[idx::table_picker::ENTER_SELECT].as_hint(),
                TABLE_PICKER_KEYS[idx::table_picker::TYPE_FILTER].as_hint(),
                TABLE_PICKER_KEYS[idx::table_picker::NAVIGATE].as_hint(),
                TABLE_PICKER_KEYS[idx::table_picker::ESC_CLOSE].as_hint(),
            ],
            InputMode::CommandPalette => {
                vec![
                    COMMAND_PALETTE_KEYS[idx::cmd_palette::ENTER_EXECUTE].as_hint(),
                    COMMAND_PALETTE_KEYS[idx::cmd_palette::NAVIGATE_JK].as_hint(),
                    COMMAND_PALETTE_KEYS[idx::cmd_palette::ESC_CLOSE].as_hint(),
                ]
            }
            InputMode::Help => vec![
                HELP_KEYS[idx::help::SCROLL].as_hint(),
                HELP_KEYS[idx::help::CLOSE].as_hint(),
                HELP_KEYS[idx::help::QUIT].as_hint(),
            ],
            InputMode::SqlModal => vec![
                SQL_MODAL_KEYS[idx::sql_modal::RUN].as_hint(),
                SQL_MODAL_KEYS[idx::sql_modal::MOVE].as_hint(),
                SQL_MODAL_KEYS[idx::sql_modal::ESC_CLOSE].as_hint(),
            ],
            InputMode::ConnectionSetup => vec![
                CONNECTION_SETUP_KEYS[idx::conn_setup::SAVE].as_hint(),
                CONNECTION_SETUP_KEYS[idx::conn_setup::TAB_NEXT].as_hint(),
                CONNECTION_SETUP_KEYS[idx::conn_setup::TAB_PREV].as_hint(),
                CONNECTION_SETUP_KEYS[idx::conn_setup::ESC_CANCEL].as_hint(),
            ],
            InputMode::ConnectionError => vec![
                CONNECTION_ERROR_KEYS[idx::conn_error::EDIT].as_hint(),
                CONNECTION_ERROR_KEYS[idx::conn_error::DETAILS].as_hint(),
                CONNECTION_ERROR_KEYS[idx::conn_error::COPY].as_hint(),
                CONNECTION_ERROR_KEYS[idx::conn_error::ESC_CLOSE].as_hint(),
                CONNECTION_ERROR_KEYS[idx::conn_error::QUIT].as_hint(),
            ],
            InputMode::ConfirmDialog => vec![
                CONFIRM_DIALOG_KEYS[idx::confirm::YES].as_hint(),
                CONFIRM_DIALOG_KEYS[idx::confirm::NO].as_hint(),
            ],
            InputMode::ConnectionSelector => vec![
                ("Enter", "Confirm"),
                ("↑/↓", "Select"),
                ("n", "New"),
                ("q", "Quit"),
            ],
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
