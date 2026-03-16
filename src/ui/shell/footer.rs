use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::er_state::ErStatus;
use crate::app::input_mode::InputMode;
use crate::app::keybindings::{
    CELL_EDIT_KEYS, COMMAND_PALETTE_ROWS, CONNECTION_ERROR_ROWS, CONNECTION_SELECTOR_ROWS,
    CONNECTION_SETUP_KEYS, ER_PICKER_ROWS, FOOTER_NAV_KEYS, GLOBAL_KEYS, HELP_ROWS, HISTORY_KEYS,
    INSPECTOR_DDL_KEYS, OVERLAY_KEYS, QUERY_HISTORY_PICKER_ROWS, RESULT_ACTIVE_KEYS,
    SQL_MODAL_CONFIRMING_KEYS, SQL_MODAL_KEYS, TABLE_PICKER_ROWS, idx,
};
use crate::app::sql_modal_context::SqlModalStatus;
use crate::app::state::AppState;
use crate::app::ui_state::ResultNavMode;
use crate::ui::primitives::atoms::spinner_char;
use crate::ui::primitives::atoms::status_message::{MessageType, StatusMessage};
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
        Line::from(Span::styled(text, Style::default().fg(Theme::TEXT_ACCENT)))
    }

    // Hint ordering: Actions → Navigation → Help → Close/Cancel → Quit
    fn get_context_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
        use crate::app::focused_pane::FocusedPane;

        match state.input_mode() {
            InputMode::Normal => {
                if state.query.is_history_mode() {
                    return vec![
                        HISTORY_KEYS[idx::history::NAV].as_hint(),
                        GLOBAL_KEYS[idx::global::HELP].as_hint(),
                        HISTORY_KEYS[idx::history::EXIT].as_hint(),
                    ];
                }

                let result_navigation =
                    state.ui.focus_mode || state.ui.focused_pane == FocusedPane::Result;
                let nav_mode = state.result_interaction.selection().mode();

                if result_navigation && nav_mode == ResultNavMode::CellActive {
                    if state.result_interaction.cell_edit().has_pending_draft() {
                        vec![
                            RESULT_ACTIVE_KEYS[idx::result_active::EDIT].as_hint(),
                            CELL_EDIT_KEYS[idx::cell_edit::WRITE].as_hint(),
                            GLOBAL_KEYS[idx::global::HELP].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::DRAFT_DISCARD].as_hint(),
                            GLOBAL_KEYS[idx::global::QUIT].as_hint(),
                        ]
                    } else {
                        // Actions → Navigation → Help → Close/Cancel → Quit
                        vec![
                            RESULT_ACTIVE_KEYS[idx::result_active::EDIT].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::YANK].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::CELL_NAV].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ROW_NAV].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::TOP_BOTTOM].as_hint(),
                            GLOBAL_KEYS[idx::global::HELP].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK].as_hint(),
                            GLOBAL_KEYS[idx::global::QUIT].as_hint(),
                        ]
                    }
                } else if result_navigation && nav_mode == ResultNavMode::RowActive {
                    if !state.result_interaction.staged_delete_rows().is_empty() {
                        // Staged-delete mode: actions relevant to committing/undoing the staged delete
                        // Actions → Navigation → Help → Close/Cancel → Quit
                        vec![
                            RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE].as_hint(),
                            CELL_EDIT_KEYS[idx::cell_edit::WRITE].as_hint(),
                            GLOBAL_KEYS[idx::global::HELP].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK].as_hint(),
                            GLOBAL_KEYS[idx::global::QUIT].as_hint(),
                        ]
                    } else {
                        // Normal RowActive mode
                        // Actions → Navigation → Help → Close/Cancel → Quit
                        vec![
                            RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ROW_YANK].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ROW_NAV].as_hint(),
                            FOOTER_NAV_KEYS[idx::footer_nav::H_SCROLL].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::TOP_BOTTOM].as_hint(),
                            GLOBAL_KEYS[idx::global::HELP].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK].as_hint(),
                            GLOBAL_KEYS[idx::global::QUIT].as_hint(),
                        ]
                    }
                } else if state.ui.focus_mode {
                    // Actions → Navigation → Help → Close/Cancel → Quit
                    let mut list =
                        vec![RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN].as_hint()];
                    if state.query.visible_result().is_some_and(|r| !r.is_error()) {
                        list.push(GLOBAL_KEYS[idx::global::CSV_EXPORT].as_hint());
                    }
                    if state.query.can_paginate_visible_result() {
                        list.push(FOOTER_NAV_KEYS[idx::footer_nav::PAGE_NAV].as_hint());
                    }
                    list.push(GLOBAL_KEYS[idx::global::HELP].as_hint());
                    list.push(GLOBAL_KEYS[idx::global::EXIT_FOCUS].as_hint());
                    list.push(GLOBAL_KEYS[idx::global::QUIT].as_hint());
                    list
                } else {
                    // Actions → Navigation → Help → Close/Cancel → Quit
                    let mut list = vec![
                        GLOBAL_KEYS[idx::global::RELOAD].as_hint(),
                        GLOBAL_KEYS[idx::global::SQL].as_hint(),
                        GLOBAL_KEYS[idx::global::ER_DIAGRAM].as_hint(),
                    ];
                    if state.ui.focused_pane == FocusedPane::Explorer {
                        list.push(GLOBAL_KEYS[idx::global::CONNECTIONS].as_hint());
                    }
                    list.push(GLOBAL_KEYS[idx::global::TABLE_PICKER].as_hint());
                    list.push(GLOBAL_KEYS[idx::global::QUERY_HISTORY].as_hint());
                    if state.connection_error.error_info.is_some() {
                        list.push(OVERLAY_KEYS[idx::overlay::ERROR_OPEN].as_hint());
                    }
                    if state.session.read_only {
                        list.push(GLOBAL_KEYS[idx::global::EXIT_READ_ONLY].as_hint());
                    } else {
                        list.push(GLOBAL_KEYS[idx::global::READ_ONLY].as_hint());
                    }
                    list.push(GLOBAL_KEYS[idx::global::FOCUS].as_hint());
                    if state.query.visible_result().is_some_and(|r| !r.is_error()) {
                        list.push(GLOBAL_KEYS[idx::global::CSV_EXPORT].as_hint());
                    }
                    if state.ui.focused_pane == FocusedPane::Inspector {
                        use crate::app::inspector_tab::InspectorTab;
                        if state.ui.inspector_tab == InspectorTab::Ddl {
                            list.push(INSPECTOR_DDL_KEYS[idx::inspector_ddl::YANK].as_hint());
                        }
                    }
                    // Navigation
                    if state.ui.focused_pane == FocusedPane::Result {
                        list.push(RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN].as_hint());
                        if state.query.can_paginate_visible_result() {
                            list.push(FOOTER_NAV_KEYS[idx::footer_nav::PAGE_NAV].as_hint());
                        }
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
            InputMode::CellEdit => vec![
                CELL_EDIT_KEYS[idx::cell_edit::WRITE].as_hint(),
                CELL_EDIT_KEYS[idx::cell_edit::TYPE].as_hint(),
                CELL_EDIT_KEYS[idx::cell_edit::MOVE].as_hint(),
                GLOBAL_KEYS[idx::global::HELP].as_hint(),
                CELL_EDIT_KEYS[idx::cell_edit::ESC_CANCEL].as_hint(),
                GLOBAL_KEYS[idx::global::QUIT].as_hint(),
            ],
            InputMode::TablePicker => vec![
                TABLE_PICKER_ROWS[idx::table_picker::ENTER_SELECT].as_hint(),
                TABLE_PICKER_ROWS[idx::table_picker::TYPE_FILTER].as_hint(),
                TABLE_PICKER_ROWS[idx::table_picker::NAVIGATE].as_hint(),
                TABLE_PICKER_ROWS[idx::table_picker::ESC_CLOSE].as_hint(),
            ],
            InputMode::CommandPalette => {
                vec![
                    COMMAND_PALETTE_ROWS[idx::cmd_palette::ENTER_EXECUTE].as_hint(),
                    COMMAND_PALETTE_ROWS[idx::cmd_palette::NAVIGATE_JK].as_hint(),
                    COMMAND_PALETTE_ROWS[idx::cmd_palette::ESC_CLOSE].as_hint(),
                ]
            }
            InputMode::Help => vec![
                HELP_ROWS[idx::help::SCROLL].as_hint(),
                HELP_ROWS[idx::help::CLOSE].as_hint(),
            ],
            InputMode::SqlModal => {
                if matches!(
                    state.sql_modal.status(),
                    SqlModalStatus::ConfirmingHigh { .. }
                ) {
                    // Modal footer already provides the authoritative hint;
                    // global footer shows only Esc.
                    vec![
                        SQL_MODAL_CONFIRMING_KEYS[idx::sql_modal_confirming::CANCEL_CONFIRM]
                            .as_hint(),
                    ]
                } else if matches!(state.sql_modal.status(), SqlModalStatus::Confirming(_)) {
                    vec![
                        SQL_MODAL_CONFIRMING_KEYS[idx::sql_modal_confirming::CONFIRM_EXECUTE]
                            .as_hint(),
                        SQL_MODAL_CONFIRMING_KEYS[idx::sql_modal_confirming::CANCEL_CONFIRM]
                            .as_hint(),
                    ]
                } else {
                    vec![
                        SQL_MODAL_KEYS[idx::sql_modal::RUN].as_hint(),
                        SQL_MODAL_KEYS[idx::sql_modal::MOVE].as_hint(),
                        SQL_MODAL_KEYS[idx::sql_modal::ESC_CLOSE].as_hint(),
                    ]
                }
            }
            InputMode::ConnectionSetup => vec![
                CONNECTION_SETUP_KEYS[idx::conn_setup::SAVE].as_hint(),
                CONNECTION_SETUP_KEYS[idx::conn_setup::TAB_NEXT].as_hint(),
                CONNECTION_SETUP_KEYS[idx::conn_setup::TAB_PREV].as_hint(),
                CONNECTION_SETUP_KEYS[idx::conn_setup::ESC_CANCEL].as_hint(),
            ],
            InputMode::ConnectionError => {
                let first = if state.session.is_service_connection() {
                    CONNECTION_ERROR_ROWS[idx::conn_error::RETRY].as_hint()
                } else {
                    CONNECTION_ERROR_ROWS[idx::conn_error::EDIT].as_hint()
                };
                vec![
                    first,
                    CONNECTION_ERROR_ROWS[idx::conn_error::SWITCH].as_hint(),
                    CONNECTION_ERROR_ROWS[idx::conn_error::DETAILS].as_hint(),
                    CONNECTION_ERROR_ROWS[idx::conn_error::COPY].as_hint(),
                    CONNECTION_ERROR_ROWS[idx::conn_error::ESC_CLOSE].as_hint(),
                ]
            }
            InputMode::ConfirmDialog => vec![],
            InputMode::ErTablePicker => vec![
                ER_PICKER_ROWS[idx::er_picker::ENTER_GENERATE].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::SELECT].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::SELECT_ALL].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::TYPE_FILTER].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::NAVIGATE].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::ESC_CLOSE].as_hint(),
            ],
            InputMode::QueryHistoryPicker => {
                use idx::query_history_picker as qh;
                vec![
                    QUERY_HISTORY_PICKER_ROWS[qh::ENTER_SELECT].as_hint(),
                    QUERY_HISTORY_PICKER_ROWS[qh::TYPE_FILTER].as_hint(),
                    QUERY_HISTORY_PICKER_ROWS[qh::NAVIGATE].as_hint(),
                    QUERY_HISTORY_PICKER_ROWS[qh::ESC_CLOSE].as_hint(),
                ]
            }
            InputMode::ConnectionSelector => {
                let r = CONNECTION_SELECTOR_ROWS;
                use idx::connection_selector as cs;
                let is_service_selected = crate::app::connection_list::is_service_selected(
                    state.connection_list_items(),
                    state.ui.connection_list_selected,
                );
                let mut list = vec![
                    r[cs::CONFIRM].as_hint(),
                    r[cs::SELECT].as_hint(),
                    r[cs::NEW].as_hint(),
                ];
                if !is_service_selected {
                    list.push(r[cs::EDIT].as_hint());
                    list.push(r[cs::DELETE].as_hint());
                }
                list.push(r[cs::CLOSE].as_hint());
                list
            }
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
                Style::default().fg(Theme::STATUS_SUCCESS),
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
