use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::er_state::ErStatus;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::model::sql_editor::modal::SqlModalStatus;
use crate::app::update::input::keybindings::{
    CELL_EDIT_KEYS, COMMAND_PALETTE_ROWS, CONNECTION_ERROR_ROWS, CONNECTION_SELECTOR_ROWS,
    CONNECTION_SETUP_KEYS, ER_PICKER_ROWS, FOOTER_NAV_KEYS, GLOBAL_KEYS, HELP_ROWS, HISTORY_KEYS,
    INSPECTOR_DDL_KEYS, JSONB_DETAIL_ROWS, JSONB_EDIT_ROWS, JSONB_SEARCH_KEYS, OVERLAY_KEYS,
    QUERY_HISTORY_PICKER_ROWS, RESULT_ACTIVE_KEYS, SQL_MODAL_CONFIRMING_KEYS, SQL_MODAL_KEYS,
    SQL_MODAL_PLAN_KEYS, TABLE_PICKER_ROWS, idx,
};
use crate::ui::primitives::atoms::key_text;
use crate::ui::primitives::atoms::spinner_char;
use crate::ui::primitives::atoms::status_message::{MessageType, StatusMessage};
use crate::ui::theme::ThemePalette;

pub struct Footer;

impl Footer {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        time_ms: Option<u128>,
        theme: &ThemePalette,
    ) {
        let base_style = Style::default().fg(theme.text_primary);
        if state.er_preparation.status == ErStatus::Waiting {
            let line = Self::build_er_waiting_line(state, time_ms, theme);
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        } else if let Some(error) = &state.messages.last_error {
            let line = StatusMessage::render_line(error, MessageType::Error, theme);
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        } else {
            // Show hints with optional inline success message
            let hints = Self::get_context_hints(state);
            let line = Self::build_hint_line_with_success(
                &hints,
                state.messages.last_success.as_deref(),
                theme,
            );
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        }
    }

    fn build_er_waiting_line(
        state: &AppState,
        time_ms: Option<u128>,
        theme: &ThemePalette,
    ) -> Line<'static> {
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

        let text = format!("{spinner} Preparing ER... ({cached}/{total})");
        Line::from(Span::styled(text, Style::default().fg(theme.text_accent)))
    }

    // Hint ordering: Actions → Navigation → Help → Close/Cancel → Quit
    fn get_context_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
        use crate::app::model::shared::focused_pane::FocusedPane;

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
                    state.ui.is_focus_mode() || state.ui.focused_pane == FocusedPane::Result;
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
                    } else if state.result_interaction.staged_delete_rows().is_empty() {
                        vec![
                            RESULT_ACTIVE_KEYS[idx::result_active::EDIT].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::YANK].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ROW_YANK].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE].as_hint(),
                            GLOBAL_KEYS[idx::global::HELP].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK].as_hint(),
                            GLOBAL_KEYS[idx::global::QUIT].as_hint(),
                        ]
                    } else {
                        vec![
                            RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE].as_hint(),
                            CELL_EDIT_KEYS[idx::cell_edit::WRITE].as_hint(),
                            GLOBAL_KEYS[idx::global::HELP].as_hint(),
                            RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK].as_hint(),
                            GLOBAL_KEYS[idx::global::QUIT].as_hint(),
                        ]
                    }
                } else if state.ui.is_focus_mode() {
                    // Actions → Navigation → Help → Close/Cancel → Quit
                    let mut list =
                        vec![RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN].as_hint()];
                    if !state.result_interaction.staged_delete_rows().is_empty() {
                        list.push(RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE].as_hint());
                        list.push(CELL_EDIT_KEYS[idx::cell_edit::WRITE].as_hint());
                    }
                    if state.can_request_csv_export() {
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
                    if state.can_request_csv_export() {
                        list.push(GLOBAL_KEYS[idx::global::CSV_EXPORT].as_hint());
                    }
                    if state.ui.focused_pane == FocusedPane::Inspector {
                        use crate::app::model::shared::inspector_tab::InspectorTab;
                        if state.ui.inspector_tab == InspectorTab::Ddl {
                            list.push(INSPECTOR_DDL_KEYS[idx::inspector_ddl::YANK].as_hint());
                        }
                    }
                    // Navigation
                    if state.ui.focused_pane == FocusedPane::Result {
                        list.push(RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN].as_hint());
                        if !state.result_interaction.staged_delete_rows().is_empty() {
                            list.push(
                                RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE].as_hint(),
                            );
                            list.push(CELL_EDIT_KEYS[idx::cell_edit::WRITE].as_hint());
                        }
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
                TABLE_PICKER_ROWS[idx::table_picker::ESC_CLOSE].as_hint(),
            ],
            InputMode::CommandPalette => {
                vec![
                    COMMAND_PALETTE_ROWS[idx::cmd_palette::ENTER_EXECUTE].as_hint(),
                    COMMAND_PALETTE_ROWS[idx::cmd_palette::ESC_CLOSE].as_hint(),
                ]
            }
            InputMode::Help => vec![HELP_ROWS[idx::help::CLOSE].as_hint()],
            InputMode::ConfirmDialog => vec![],
            InputMode::SqlModal => {
                if matches!(
                    state.sql_modal.status(),
                    SqlModalStatus::ConfirmingHigh { .. }
                ) {
                    vec![
                        SQL_MODAL_CONFIRMING_KEYS[idx::sql_modal_confirming::CANCEL_CONFIRM]
                            .as_hint(),
                    ]
                } else if matches!(
                    state.sql_modal.status(),
                    SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error
                ) {
                    // Hints are shown on the modal's bottom border, not the main footer.
                    vec![]
                } else {
                    // Editing mode
                    vec![
                        SQL_MODAL_KEYS[idx::sql_modal::RUN].as_hint(),
                        SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::EXPLAIN].as_hint(),
                        SQL_MODAL_KEYS[idx::sql_modal::MOVE].as_hint(),
                        SQL_MODAL_KEYS[idx::sql_modal::ESC_NORMAL].as_hint(),
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
            InputMode::ErTablePicker => vec![
                ER_PICKER_ROWS[idx::er_picker::ENTER_GENERATE].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::SELECT].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::SELECT_ALL].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::TYPE_FILTER].as_hint(),
                ER_PICKER_ROWS[idx::er_picker::ESC_CLOSE].as_hint(),
            ],
            InputMode::QueryHistoryPicker => vec![
                QUERY_HISTORY_PICKER_ROWS[idx::qh_picker::ENTER_SELECT].as_hint(),
                QUERY_HISTORY_PICKER_ROWS[idx::qh_picker::TYPE_FILTER].as_hint(),
                QUERY_HISTORY_PICKER_ROWS[idx::qh_picker::ESC_CLOSE].as_hint(),
            ],
            InputMode::JsonbDetail => {
                if state.jsonb_detail.search().active {
                    vec![
                        JSONB_SEARCH_KEYS[idx::jsonb_search::TYPE_SEARCH].as_hint(),
                        JSONB_SEARCH_KEYS[idx::jsonb_search::CONFIRM].as_hint(),
                        JSONB_SEARCH_KEYS[idx::jsonb_search::CANCEL].as_hint(),
                    ]
                } else {
                    vec![
                        JSONB_DETAIL_ROWS[idx::jsonb_detail::YANK].as_hint(),
                        JSONB_DETAIL_ROWS[idx::jsonb_detail::INSERT].as_hint(),
                        JSONB_DETAIL_ROWS[idx::jsonb_detail::SEARCH].as_hint(),
                        JSONB_DETAIL_ROWS[idx::jsonb_detail::NEXT_PREV].as_hint(),
                        JSONB_DETAIL_ROWS[idx::jsonb_detail::MOVE].as_hint(),
                        JSONB_DETAIL_ROWS[idx::jsonb_detail::CLOSE].as_hint(),
                    ]
                }
            }
            InputMode::JsonbEdit => vec![
                JSONB_EDIT_ROWS[idx::jsonb_edit::ESC_NORMAL].as_hint(),
                JSONB_EDIT_ROWS[idx::jsonb_edit::MOVE].as_hint(),
                JSONB_EDIT_ROWS[idx::jsonb_edit::HOME_END].as_hint(),
            ],
            InputMode::ConnectionSelector => {
                let r = CONNECTION_SELECTOR_ROWS;
                use idx::connection_selector as cs;
                let is_service_selected = crate::app::model::connection::list::is_service_selected(
                    state.connection_list_items(),
                    state.ui.connection_list_selected,
                );
                let mut list = vec![r[cs::CONFIRM].as_hint(), r[cs::NEW].as_hint()];
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
        theme: &ThemePalette,
    ) -> Line<'static> {
        let mut spans = Vec::new();

        if let Some(msg) = success_msg {
            spans.push(Span::styled(
                format!("✓ {msg}  "),
                Style::default().fg(theme.status_success),
            ));
        }

        for (i, (key, desc)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(key_text(key, theme));
            spans.push(Span::raw(format!(":{desc}")));
        }

        Line::from(spans)
    }
}
