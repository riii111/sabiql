use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::connection::setup::ConnectionField;
use crate::app::model::er_state::ErStatus;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::settings::KeymapPreset;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::model::sql_editor::modal::SqlModalStatus;
use crate::app::update::input::keybindings::{
    cell_edit, command_palette, command_palette as command_palette_key, connection_error,
    connection_selector, connection_setup, connection_setup_save, csv_export, er_picker,
    er_picker_select_all, exit_read_only, footer_nav, global, help, inspector_ddl, jsonb_detail,
    jsonb_edit, jsonb_search, overlay, query_history, query_history_picker, read_only,
    result_active, settings, sql_modal, sql_modal_confirming, sql_modal_plan, table_picker,
    table_picker as table_picker_key,
};
use crate::features::settings::hints::settings_hints;
use crate::primitives::atoms::key_text;
use crate::primitives::atoms::spinner_char;
use crate::primitives::atoms::status_message::{MessageType, StatusMessage};
use crate::theme::ThemePalette;

pub struct Footer;

impl Footer {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        time_ms: Option<u128>,
        theme: &ThemePalette,
    ) {
        let base_style = Style::default().fg(theme.semantic.text.primary);
        if state.er_preparation.status() == ErStatus::Waiting {
            let line = Self::build_er_waiting_line(state, time_ms, theme);
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        } else if let Some(error) = state.messages.last_error() {
            let line = StatusMessage::render_line(error, MessageType::Error, theme);
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        } else {
            // Show hints with optional inline success message
            let hints = Self::get_context_hints(state);
            let line =
                Self::build_hint_line_with_success(&hints, state.messages.last_success(), theme);
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
                .map_or(0, |d| d.as_millis())
        });
        let spinner = spinner_char(now_ms);

        let progress = state.er_preparation.progress();

        let text = format!(
            "{spinner} Preparing ER... ({}/{})",
            progress.cached, progress.total
        );
        Line::from(Span::styled(
            text,
            Style::default().fg(theme.semantic.text.accent),
        ))
    }

    // Hint ordering: Actions → Navigation → Help → Close/Cancel → Quit
    fn get_context_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
        use crate::app::model::shared::focused_pane::FocusedPane;

        match state.input_mode() {
            InputMode::Normal => {
                let keymap_preset = state.settings.saved_keymap_preset();
                let result_navigation =
                    state.ui.is_focus_mode() || state.ui.focused_pane() == FocusedPane::Result;
                let nav_mode = state.result_interaction.selection().mode();

                if result_navigation && nav_mode == ResultNavMode::CellActive {
                    if state.result_interaction.cell_edit().has_pending_draft() {
                        vec![
                            result_active::EDIT.as_hint(),
                            cell_edit::WRITE.as_hint(),
                            global::HELP.as_hint(),
                            result_active::DRAFT_DISCARD.as_hint(),
                            global::QUIT.as_hint(),
                        ]
                    } else if state.result_interaction.staged_delete_rows().is_empty() {
                        vec![
                            result_active::EDIT.as_hint(),
                            result_active::YANK.as_hint(),
                            result_active::ROW_YANK.as_hint(),
                            result_active::STAGE_DELETE.as_hint(),
                            global::HELP.as_hint(),
                            result_active::ESC_BACK.as_hint(),
                            global::QUIT.as_hint(),
                        ]
                    } else {
                        vec![
                            result_active::STAGE_DELETE.as_hint(),
                            result_active::UNSTAGE_DELETE.as_hint(),
                            cell_edit::WRITE.as_hint(),
                            global::HELP.as_hint(),
                            result_active::ESC_BACK.as_hint(),
                            global::QUIT.as_hint(),
                        ]
                    }
                } else if state.ui.is_focus_mode() {
                    // Actions → Navigation → Help → Close/Cancel → Quit
                    let mut list = vec![result_active::ENTER_DEEPEN.as_hint()];
                    if !state.result_interaction.staged_delete_rows().is_empty() {
                        list.push(result_active::UNSTAGE_DELETE.as_hint());
                        list.push(cell_edit::WRITE.as_hint());
                    }
                    if state.can_request_csv_export() {
                        list.push(csv_export(keymap_preset).as_hint());
                    }
                    if state.query.can_paginate_visible_result() {
                        list.push(footer_nav::PAGE_NAV.as_hint());
                    }
                    list.push(global::HELP.as_hint());
                    list.push(settings(keymap_preset).as_hint());
                    list.push(global::EXIT_FOCUS.as_hint());
                    list.push(global::QUIT.as_hint());
                    list
                } else {
                    // Actions → Navigation → Help → Close/Cancel → Quit
                    let capabilities = state.session.active_db_capabilities();
                    let active_inspector_tab =
                        capabilities.normalize_inspector_tab(state.ui.inspector_tab());
                    let mut list = vec![
                        global::RELOAD.as_hint(),
                        global::SQL.as_hint(),
                        global::ER_DIAGRAM.as_hint(),
                    ];
                    if state.ui.focused_pane() == FocusedPane::Explorer {
                        list.push(global::CONNECTIONS.as_hint());
                    }
                    list.push(table_picker_key(keymap_preset).as_hint());
                    list.push(query_history(keymap_preset).as_hint());
                    if state.connection_error.has_error() {
                        list.push(overlay::ERROR_OPEN.as_hint());
                    }
                    if state.session.is_read_only() {
                        list.push(exit_read_only(keymap_preset).as_hint());
                    } else {
                        list.push(read_only(keymap_preset).as_hint());
                    }
                    list.push(global::FOCUS.as_hint());
                    if state.can_request_csv_export() {
                        list.push(csv_export(keymap_preset).as_hint());
                    }
                    if state.ui.focused_pane() == FocusedPane::Inspector {
                        use crate::app::model::shared::inspector_tab::InspectorTab;
                        if active_inspector_tab == InspectorTab::Ddl {
                            list.push(inspector_ddl::YANK.as_hint());
                        }
                    }
                    // Navigation
                    if state.ui.focused_pane() == FocusedPane::Result {
                        list.push(result_active::ENTER_DEEPEN.as_hint());
                        if !state.result_interaction.staged_delete_rows().is_empty() {
                            list.push(result_active::UNSTAGE_DELETE.as_hint());
                            list.push(cell_edit::WRITE.as_hint());
                        }
                        if state.query.can_paginate_visible_result() {
                            list.push(footer_nav::PAGE_NAV.as_hint());
                        }
                    }
                    if state.ui.focused_pane() == FocusedPane::Inspector
                        && capabilities.supported_inspector_tabs().len() > 1
                    {
                        list.push(global::INSPECTOR_TABS.as_hint());
                    }
                    list.push(global::HELP.as_hint());
                    list.push(command_palette_key(keymap_preset).as_hint());
                    list.push(settings(keymap_preset).as_hint());
                    list.push(global::QUIT.as_hint());
                    list
                }
            }
            InputMode::CommandLine => vec![
                overlay::ENTER_EXECUTE.as_hint(),
                overlay::ESC_CANCEL.as_hint(),
            ],
            InputMode::CellEdit => vec![
                cell_edit::WRITE.as_hint(),
                cell_edit::TYPE.as_hint(),
                cell_edit::MOVE.as_hint(),
                global::HELP.as_hint(),
                cell_edit::ESC_CANCEL.as_hint(),
                global::QUIT.as_hint(),
            ],
            InputMode::TablePicker => vec![
                table_picker::ENTER_SELECT.as_hint(),
                table_picker::TYPE_FILTER.as_hint(),
                table_picker::ESC_CLOSE.as_hint(),
            ],
            InputMode::CommandPalette => {
                vec![
                    command_palette::ENTER_EXECUTE.as_hint(),
                    command_palette::ESC_CLOSE.as_hint(),
                ]
            }
            InputMode::Help => vec![help::H_SCROLL.as_hint(), help::CLOSE.as_hint()],
            InputMode::Settings => settings_hints(state),
            InputMode::ConfirmDialog => vec![],
            InputMode::SqlModal => {
                if matches!(
                    state.sql_modal.status(),
                    SqlModalStatus::ConfirmingRisk { .. }
                ) {
                    vec![
                        sql_modal_confirming::ENTER_EXECUTE.as_hint(),
                        sql_modal_confirming::CANCEL_CONFIRM.as_hint(),
                    ]
                } else if matches!(
                    state.sql_modal.status(),
                    SqlModalStatus::ConfirmingHigh { .. }
                ) {
                    vec![sql_modal_confirming::CANCEL_CONFIRM.as_hint()]
                } else if matches!(
                    state.sql_modal.status(),
                    SqlModalStatus::Normal
                        | SqlModalStatus::Success
                        | SqlModalStatus::Error
                        | SqlModalStatus::ConfirmingAnalyzeHigh { .. }
                        | SqlModalStatus::ConfirmingAnalyzeRisk { .. }
                ) {
                    // Hints are shown on the modal's bottom border, not the main footer.
                    vec![]
                } else {
                    // Editing / Running
                    let mut hints = vec![
                        sql_modal::RUN.as_hint(),
                        sql_modal::MOVE.as_hint(),
                        sql_modal::ESC_NORMAL.as_hint(),
                    ];
                    if state.session.active_db_capabilities().supports_explain()
                        && state.sql_modal.status() == &SqlModalStatus::Editing
                        && state.settings.saved_keymap_preset() == KeymapPreset::Default
                    {
                        hints.insert(1, sql_modal_plan::EXPLAIN.as_hint());
                    }
                    hints
                }
            }
            InputMode::ConnectionSetup => {
                let submit_hint = if matches!(
                    state.connection_setup.focused_field(),
                    ConnectionField::DatabaseType | ConnectionField::SslMode
                ) {
                    connection_setup::ENTER_DROPDOWN.as_hint()
                } else {
                    connection_setup_save(state.settings.saved_keymap_preset()).as_hint()
                };
                vec![
                    submit_hint,
                    connection_setup::TAB_NEXT.as_hint(),
                    connection_setup::TAB_PREV.as_hint(),
                    connection_setup::ESC_CANCEL.as_hint(),
                ]
            }
            InputMode::ConnectionError => {
                let first = if state.session.is_service_connection() {
                    connection_error::RETRY.as_hint()
                } else {
                    connection_error::EDIT.as_hint()
                };
                vec![
                    first,
                    connection_error::SWITCH.as_hint(),
                    connection_error::DETAILS.as_hint(),
                    connection_error::COPY.as_hint(),
                    connection_error::ESC_CLOSE.as_hint(),
                ]
            }
            InputMode::ErTablePicker => vec![
                er_picker::ENTER_GENERATE.as_hint(),
                er_picker::SELECT.as_hint(),
                er_picker_select_all(state.settings.saved_keymap_preset()).as_hint(),
                er_picker::TYPE_FILTER.as_hint(),
                er_picker::ESC_CLOSE.as_hint(),
            ],
            InputMode::QueryHistoryPicker => vec![
                query_history_picker::ENTER_SELECT.as_hint(),
                query_history_picker::TYPE_FILTER.as_hint(),
                query_history_picker::ESC_CLOSE.as_hint(),
            ],
            InputMode::JsonbDetail => {
                if state.jsonb_detail.search().is_active() {
                    vec![
                        jsonb_search::TYPE_SEARCH.as_hint(),
                        jsonb_search::CONFIRM.as_hint(),
                        jsonb_search::CANCEL.as_hint(),
                    ]
                } else {
                    vec![
                        jsonb_detail::YANK.as_hint(),
                        jsonb_detail::INSERT.as_hint(),
                        jsonb_detail::SEARCH.as_hint(),
                        jsonb_detail::NEXT_PREV.as_hint(),
                        jsonb_detail::MOVE.as_hint(),
                        jsonb_detail::CLOSE.as_hint(),
                    ]
                }
            }
            InputMode::JsonbEdit => vec![
                jsonb_edit::ESC_NORMAL.as_hint(),
                jsonb_edit::MOVE.as_hint(),
                jsonb_edit::HOME_END.as_hint(),
            ],
            InputMode::ConnectionSelector => {
                use connection_selector as cs;
                let is_service_selected = crate::app::model::connection::list::is_service_selected(
                    state.connection_list_items(),
                    state.ui.connection_list_selected(),
                );
                let mut list = vec![cs::CONFIRM.as_hint(), cs::NEW.as_hint()];
                if !is_service_selected {
                    list.push(cs::EDIT.as_hint());
                    list.push(cs::DELETE.as_hint());
                }
                list.push(cs::CLOSE.as_hint());
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
                Style::default().fg(theme.semantic.status.success),
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

#[cfg(test)]
mod tests {
    use super::Footer;
    use crate::app::domain::{ConnectionId, DatabaseType};
    use crate::app::model::app_state::AppState;
    use crate::app::model::connection::setup::ConnectionField;
    use crate::app::model::shared::focused_pane::FocusedPane;
    use crate::app::model::shared::input_mode::InputMode;
    use crate::app::model::shared::settings::KeymapPreset;
    use crate::app::model::sql_editor::modal::SqlModalStatus;
    use crate::app::update::input::keybindings::{connection_setup, global};
    use rstest::rstest;

    fn inspector_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);
        state.ui.set_focused_pane(FocusedPane::Inspector);
        state
    }

    fn focus_connection_field(state: &mut AppState, field: ConnectionField) {
        while state.connection_setup.focused_field() != field {
            state.connection_setup.focus_next_field();
        }
    }

    #[rstest]
    #[case(None, false)]
    #[case(Some(DatabaseType::SQLite), true)]
    fn inspector_tabs_hint_visibility_tracks_supported_tab_count(
        #[case] database_type: Option<DatabaseType>,
        #[case] expected_visible: bool,
    ) {
        let mut state = inspector_state();
        if let Some(database_type) = database_type {
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "database",
                database_type,
                "sqlite://test.db",
            );
        }

        let hints = Footer::get_context_hints(&state);

        assert_eq!(
            hints.contains(&global::INSPECTOR_TABS.as_hint()),
            expected_visible
        );
    }

    #[test]
    fn settings_custom_browser_hint_shows_edit_when_selected() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Settings);
        state.settings.switch_next_section();
        state.settings.switch_next_section();
        state.settings.start_custom_browser_edit();
        state.settings.stop_custom_browser_edit();

        let hints = Footer::get_context_hints(&state);

        assert!(hints.contains(&("i", "Edit")));
        assert!(hints.contains(&("Tab/⇧Tab", "Section")));
        assert!(hints.contains(&("Esc", "Cancel")));
    }

    #[test]
    fn settings_custom_browser_edit_hint_shows_done_and_typing() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Settings);
        state.settings.switch_next_section();
        state.settings.switch_next_section();
        state.settings.start_custom_browser_edit();

        let hints = Footer::get_context_hints(&state);

        assert_eq!(
            hints,
            vec![("Enter", "Apply"), ("Esc", "Done"), ("Type", "Browser")]
        );
    }

    #[test]
    fn ide_sql_editing_footer_omits_explain_hint() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/test",
        );
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.set_status_for_test(SqlModalStatus::Editing);
        state.settings.load_keymap_preset(KeymapPreset::Ide);

        let hints = Footer::get_context_hints(&state);

        assert!(!hints.contains(&("^E", "Explain")));
    }

    #[test]
    fn connection_setup_footer_shows_toggle_on_ssl_field() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::ConnectionSetup);
        focus_connection_field(&mut state, ConnectionField::SslMode);

        let hints = Footer::get_context_hints(&state);

        assert!(hints.contains(&connection_setup::ENTER_DROPDOWN.as_hint()));
        assert!(!hints.contains(&("Enter", "Connect")));
    }
}
