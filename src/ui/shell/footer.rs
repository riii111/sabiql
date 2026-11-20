use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::browse::json_detail::JsonDetailMode;
use crate::app::model::er_state::ErStatus;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::model::sql_editor::modal::SqlModalStatus;
use crate::app::policy::{FeaturePolicy, FeatureRequirement};
use crate::app::update::input::keybindings::{
    cell_edit, command_palette as command_palette_key, csv_export, exit_read_only, footer_nav,
    global, inspector_ddl, overlay, query_history, read_only, result_active, settings,
    sqlite_diagnostics, table_picker as table_picker_key,
};
use crate::app::update::input::keymap::{
    HintSurface, KeyContext, Picker, cell_detail_hints, cell_edit_hints, command_line_hints,
    connection_error_hints, json_edit_hints, json_search_hints, json_view_hints, picker_hints,
    settings_hints, sql_edit_hints, sqlite_diagnostics_hints,
};
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
            let line = StatusMessage::render_lines(error, MessageType::Error, area.width, theme)
                .into_iter()
                .next()
                .unwrap_or_else(|| StatusMessage::render_line("", MessageType::Error, theme));
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        } else {
            // Show hints with optional inline success message
            let context = KeyContext::from_state(state);
            let derived = match state.input_mode() {
                InputMode::TablePicker => {
                    Some(picker_hints(Picker::Table, HintSurface::Footer, &context))
                }
                InputMode::ErTablePicker => {
                    Some(picker_hints(Picker::Er, HintSurface::Footer, &context))
                }
                InputMode::SqliteDiagnostics => {
                    Some(sqlite_diagnostics_hints(HintSurface::Footer, &context))
                }
                InputMode::CellDetail => Some(cell_detail_hints(HintSurface::Footer, &context)),
                InputMode::ConnectionError => {
                    Some(connection_error_hints(HintSurface::Footer, &context))
                }
                InputMode::Settings => Some(settings_hints(HintSurface::Footer, &context)),
                InputMode::CommandLine => Some(command_line_hints(HintSurface::Footer, &context)),
                InputMode::CellEdit => Some(cell_edit_hints(HintSurface::Footer, &context)),
                InputMode::JsonDetail => {
                    Some(if state.json_detail.mode() == JsonDetailMode::Searching {
                        json_search_hints(HintSurface::Footer, &context)
                    } else {
                        json_view_hints(HintSurface::Footer, &context)
                    })
                }
                InputMode::JsonEdit => Some(json_edit_hints(HintSurface::Footer, &context)),
                InputMode::SqlModal
                    if matches!(state.sql_modal.status(), SqlModalStatus::Editing) =>
                {
                    Some(sql_edit_hints(HintSurface::Footer, &context))
                }
                _ => None,
            };
            let hints = if let Some(ref derived) = derived {
                derived
                    .iter()
                    .map(|hint| (hint.key.as_str(), hint.short))
                    .collect()
            } else {
                Self::get_context_hints(state)
            };
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

        let progress = state.er_preparation.progress(&state.table_prefetch);

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
    fn get_context_hints(state: &AppState) -> Vec<(&str, &str)> {
        use crate::app::model::shared::focused_pane::FocusedPane;

        match state.input_mode() {
            InputMode::Normal => {
                let keymap_preset = state.settings.saved_keymap_preset();
                let result_navigation =
                    state.ui.is_focus_mode() || state.ui.focused_pane() == FocusedPane::Result;
                let nav_mode = state.result_interaction.selection().mode();

                if result_navigation && nav_mode == ResultNavMode::CellActive {
                    let can_write_preview = state.can_write_visible_preview();
                    let can_edit_selected_cell = state.can_edit_selected_cell();
                    if state.result_interaction.cell_edit().has_pending_draft() {
                        vec![
                            result_active::EDIT.as_hint(),
                            cell_edit::WRITE.as_hint(),
                            global::CONNECTIONS.as_hint(),
                            global::HELP.as_hint(),
                            result_active::DRAFT_DISCARD.as_hint(),
                            global::QUIT.as_hint(),
                        ]
                    } else if state.result_interaction.staged_delete_rows().is_empty() {
                        let mut hints = vec![result_active::DETAIL.as_hint()];
                        if can_edit_selected_cell {
                            hints.push(result_active::EDIT.as_hint());
                        }
                        hints.extend([
                            result_active::YANK.as_hint(),
                            result_active::ROW_YANK.as_hint(),
                            result_active::ROW_DETAIL.as_hint(),
                        ]);
                        if can_write_preview {
                            hints.push(result_active::STAGE_DELETE.as_hint());
                        }
                        hints.extend([
                            global::CONNECTIONS.as_hint(),
                            global::HELP.as_hint(),
                            result_active::ESC_BACK.as_hint(),
                            global::QUIT.as_hint(),
                        ]);
                        hints
                    } else {
                        vec![
                            result_active::STAGE_DELETE.as_hint(),
                            result_active::UNSTAGE_DELETE.as_hint(),
                            cell_edit::WRITE.as_hint(),
                            global::CONNECTIONS.as_hint(),
                            global::HELP.as_hint(),
                            result_active::ESC_BACK.as_hint(),
                            global::QUIT.as_hint(),
                        ]
                    }
                } else if state.ui.is_focus_mode() {
                    // Actions → Navigation → Help → Close/Cancel → Quit
                    let mut list = vec![result_active::ENTER_DEEPEN.as_hint()];
                    list.push(global::CONNECTIONS.as_hint());
                    if !state.result_interaction.staged_delete_rows().is_empty() {
                        list.push(result_active::UNSTAGE_DELETE.as_hint());
                        list.push(result_active::CLEAR_STAGED_DELETE.as_hint());
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
                    let capabilities = state.session.active_engine_feature_profile();
                    let feature_policy = FeaturePolicy::new(&capabilities);
                    let active_inspector_tab =
                        capabilities.normalize_inspector_tab(state.ui.inspector_tab());
                    let mut list = vec![global::SQL.as_hint()];
                    if state.session.dsn().is_some() {
                        list.insert(0, global::RELOAD.as_hint());
                    }
                    if feature_policy.is_enabled(FeatureRequirement::ErDiagram) {
                        list.push(global::ER_DIAGRAM.as_hint());
                    }
                    if feature_policy.is_enabled(FeatureRequirement::SqliteDiagnostics) {
                        list.push(sqlite_diagnostics(keymap_preset).as_hint());
                    }
                    list.push(global::CONNECTIONS.as_hint());
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
                            list.push(result_active::CLEAR_STAGED_DELETE.as_hint());
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
            InputMode::TablePicker
            | InputMode::CommandPalette
            | InputMode::Help
            | InputMode::ConfirmDialog
            | InputMode::ConnectionSetup
            | InputMode::RowDetail
            | InputMode::ConnectionSelector
            | InputMode::ConnectionError
            | InputMode::Settings
            | InputMode::CommandLine
            | InputMode::CellEdit
            | InputMode::JsonEdit
            | InputMode::JsonDetail
            | InputMode::QueryHistoryPicker
            | InputMode::ErTablePicker
            | InputMode::SqliteDiagnostics
            | InputMode::SqlModal
            | InputMode::CellDetail => vec![],
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
    use crate::app::model::connection::error::ConnectionErrorInfo;
    use crate::app::model::connection::setup::ConnectionField;
    use crate::app::model::shared::focused_pane::FocusedPane;
    use crate::app::model::shared::input_mode::InputMode;
    use crate::app::model::shared::settings::KeymapPreset;
    use crate::app::model::shared::ui_state::FocusMode;
    use crate::app::ports::outbound::DbOperationError;
    use crate::app::update::input::keybindings::{global, result_active};
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

    fn result_focused_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);
        state.ui.set_focused_pane(FocusedPane::Result);
        state
    }

    #[rstest]
    #[case(FocusedPane::Explorer)]
    #[case(FocusedPane::Inspector)]
    #[case(FocusedPane::Result)]
    fn normal_footer_shows_connections_for_every_pane(#[case] focused_pane: FocusedPane) {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);
        state.ui.set_focused_pane(focused_pane);

        let hints = Footer::get_context_hints(&state);

        assert!(hints.contains(&global::CONNECTIONS.as_hint()));
    }

    #[test]
    fn unconnected_normal_footer_hides_reload() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);

        let hints = Footer::get_context_hints(&state);

        assert!(!hints.contains(&global::RELOAD.as_hint()));
        assert!(hints.contains(&global::CONNECTIONS.as_hint()));
    }

    #[rstest]
    #[case(false)]
    #[case(true)]
    fn focus_mode_footer_shows_connections(#[case] result_active: bool) {
        let mut state = result_focused_state();
        state
            .ui
            .set_focus_mode(FocusMode::focused(FocusedPane::Explorer));
        if result_active {
            state.result_interaction.activate_cell(0, 0);
        }

        let hints = Footer::get_context_hints(&state);

        assert!(hints.contains(&global::CONNECTIONS.as_hint()));
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

    #[rstest]
    #[case(DatabaseType::PostgreSQL, true)]
    #[case(DatabaseType::SQLite, false)]
    fn er_hint_visibility_tracks_capability(
        #[case] database_type: DatabaseType,
        #[case] expected_visible: bool,
    ) {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            database_type,
            "test://database",
        );

        let hints = Footer::get_context_hints(&state);

        assert_eq!(
            hints.contains(&global::ER_DIAGRAM.as_hint()),
            expected_visible
        );
    }

    #[test]
    fn unsupported_json_modes_hide_modal_hints() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::SQLite,
            "sqlite://test.db",
        );

        state.modal.set_mode(InputMode::JsonDetail);
        assert!(
            super::json_view_hints(
                super::HintSurface::Footer,
                &super::KeyContext::from_state(&state)
            )
            .is_empty()
        );

        state.modal.set_mode(InputMode::JsonEdit);
        assert!(
            super::json_edit_hints(
                super::HintSurface::Footer,
                &super::KeyContext::from_state(&state)
            )
            .is_empty()
        );
    }

    #[test]
    fn mysql_json_detail_footer_keeps_non_modal_hints() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::MySQL,
            "mysql://test",
        );
        state.modal.set_mode(InputMode::JsonDetail);

        let hints = super::json_view_hints(
            super::HintSurface::Footer,
            &super::KeyContext::from_state(&state),
        );
        assert_eq!(
            hints.iter().map(|hint| hint.short).collect::<Vec<_>>(),
            ["Next/Prev", "Left/Right", "Down/Up"]
        );
    }

    #[test]
    fn row_detail_hint_is_hidden_in_result_scroll_mode() {
        let state = result_focused_state();

        let hints = Footer::get_context_hints(&state);

        assert!(!hints.contains(&result_active::ROW_DETAIL.as_hint()));
    }

    #[test]
    fn row_detail_hint_is_hidden_in_result_focus_scroll_mode() {
        let mut state = result_focused_state();
        state
            .ui
            .set_focus_mode(FocusMode::focused(FocusedPane::Explorer));

        let hints = Footer::get_context_hints(&state);

        assert!(!hints.contains(&result_active::ROW_DETAIL.as_hint()));
    }

    #[test]
    fn row_detail_hint_is_visible_in_cell_active_mode() {
        let mut state = result_focused_state();
        state.result_interaction.activate_cell(0, 0);

        let hints = Footer::get_context_hints(&state);

        assert!(hints.contains(&result_active::ROW_DETAIL.as_hint()));
    }

    #[test]
    fn row_detail_footer_hides_modal_hints() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::RowDetail);

        let hints = Footer::get_context_hints(&state);

        assert!(hints.is_empty());
    }

    #[test]
    fn query_history_picker_footer_hides_modal_hints() {
        let mut state = AppState::new("test".to_string());

        state.modal.set_mode(InputMode::QueryHistoryPicker);
        assert!(Footer::get_context_hints(&state).is_empty());
    }

    #[rstest]
    #[case(InputMode::ConnectionSelector)]
    #[case(InputMode::CommandPalette)]
    #[case(InputMode::ConnectionSetup)]
    fn modal_footer_hides_hints_rendered_in_modal_frame(#[case] mode: InputMode) {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(mode);

        assert!(Footer::get_context_hints(&state).is_empty());
    }

    #[test]
    fn settings_footer_keeps_only_selection_hint() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Settings);
        state.settings.switch_next_section();
        state.settings.switch_next_section();
        state.settings.start_custom_browser_edit();
        state.settings.stop_custom_browser_edit();

        let hints = super::settings_hints(
            super::HintSurface::Footer,
            &super::KeyContext::from_state(&state),
        );
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].short, "Select");
    }

    #[test]
    fn settings_custom_browser_edit_footer_hides_modal_hints() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Settings);
        state.settings.switch_next_section();
        state.settings.switch_next_section();
        state.settings.start_custom_browser_edit();

        let hints = super::settings_hints(
            super::HintSurface::Footer,
            &super::KeyContext::from_state(&state),
        );

        assert!(hints.is_empty());
    }

    #[test]
    fn help_footer_hints_follow_help_mode() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Help);

        assert_eq!(
            Footer::get_context_hints(&state),
            Vec::<(&str, &str)>::new()
        );

        state.ui.help_mut().enter_filter_editing();

        assert_eq!(
            Footer::get_context_hints(&state),
            Vec::<(&str, &str)>::new()
        );
    }

    #[rstest::rstest]
    #[case(KeymapPreset::Default)]
    #[case(KeymapPreset::Ide)]
    fn sql_editing_footer_omits_explain_hint(#[case] preset: KeymapPreset) {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/test",
        );
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.enter_editing();
        state.settings.load_keymap_preset(preset);

        let hints = super::sql_edit_hints(
            super::HintSurface::Footer,
            &super::KeyContext::from_state(&state),
        );

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].short, "Move");
    }

    #[test]
    fn connection_setup_footer_hides_modal_hints() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::ConnectionSetup);
        focus_connection_field(&mut state, ConnectionField::SslMode);

        assert!(Footer::get_context_hints(&state).is_empty());
    }

    #[test]
    fn connection_error_footer_hides_retry_for_save_and_connect_failure() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "mysql",
            DatabaseType::MySQL,
            "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
        );
        state.connection_error.set_save_and_connect_error(
            ConnectionErrorInfo::from_db_operation_error(&DbOperationError::Timeout(
                "connection timed out".to_string(),
            )),
        );
        state.modal.set_mode(InputMode::ConnectionError);

        let hints = super::connection_error_hints(
            super::HintSurface::Footer,
            &super::KeyContext::from_state(&state),
        );

        assert_eq!(hints[0].short, "Edit");
        assert!(hints.iter().all(|hint| hint.short != "Close"));
    }
}
