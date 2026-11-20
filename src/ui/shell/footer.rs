use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::er_state::ErStatus;
use crate::app::update::input::keymap::{KeyContext, footer_hints};
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
            let derived = footer_hints(&context);
            let hints: Vec<_> = derived
                .iter()
                .map(|hint| (hint.key.as_str(), hint.short))
                .collect();
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

    #[cfg(test)]
    fn get_context_hints(state: &AppState) -> Vec<(String, &'static str)> {
        footer_hints(&KeyContext::from_state(state))
            .into_iter()
            .map(|hint| (hint.key, hint.short))
            .collect()
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
    use crate::app::update::input::keymap::{
        HintSurface, KeyContext, connection_error_hints, json_edit_hints, json_view_hints,
        settings_hints, sql_edit_hints,
    };
    use rstest::rstest;

    fn contains(hints: &[(String, &str)], expected: (&str, &str)) -> bool {
        hints
            .iter()
            .any(|(key, label)| (key.as_str(), *label) == expected)
    }

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

        assert!(contains(&hints, ("c", "Connections")));
    }

    #[test]
    fn unconnected_normal_footer_hides_reload() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);

        let hints = Footer::get_context_hints(&state);

        assert!(!contains(&hints, ("r", "Reload")));
        assert!(contains(&hints, ("c", "Connections")));
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

        assert!(contains(&hints, ("c", "Connections")));
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

        assert_eq!(contains(&hints, ("Tab/⇧Tab", "InsTabs")), expected_visible);
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

        assert_eq!(contains(&hints, ("e", "ER Diagram")), expected_visible);
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
        assert!(json_view_hints(HintSurface::Footer, &KeyContext::from_state(&state)).is_empty());

        state.modal.set_mode(InputMode::JsonEdit);
        assert!(json_edit_hints(HintSurface::Footer, &KeyContext::from_state(&state)).is_empty());
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

        let hints = json_view_hints(HintSurface::Footer, &KeyContext::from_state(&state));
        assert_eq!(
            hints.iter().map(|hint| hint.short).collect::<Vec<_>>(),
            ["Next/Prev", "Left/Right", "Down/Up"]
        );
    }

    #[test]
    fn row_detail_hint_is_hidden_in_result_scroll_mode() {
        let state = result_focused_state();

        let hints = Footer::get_context_hints(&state);

        assert!(!contains(&hints, ("K", "Row Detail")));
    }

    #[test]
    fn row_detail_hint_is_hidden_in_result_focus_scroll_mode() {
        let mut state = result_focused_state();
        state
            .ui
            .set_focus_mode(FocusMode::focused(FocusedPane::Explorer));

        let hints = Footer::get_context_hints(&state);

        assert!(!contains(&hints, ("K", "Row Detail")));
    }

    #[test]
    fn row_detail_hint_is_visible_in_cell_active_mode() {
        let mut state = result_focused_state();
        state.result_interaction.activate_cell(0, 0);

        let hints = Footer::get_context_hints(&state);

        assert!(contains(&hints, ("K", "Row Detail")));
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

        let hints = settings_hints(HintSurface::Footer, &KeyContext::from_state(&state));
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

        let hints = settings_hints(HintSurface::Footer, &KeyContext::from_state(&state));

        assert!(hints.is_empty());
    }

    #[test]
    fn help_footer_hints_follow_help_mode() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Help);

        assert_eq!(
            Footer::get_context_hints(&state),
            Vec::<(String, &str)>::new()
        );

        state.ui.help_mut().enter_filter_editing();

        assert_eq!(
            Footer::get_context_hints(&state),
            Vec::<(String, &str)>::new()
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

        let hints = sql_edit_hints(HintSurface::Footer, &KeyContext::from_state(&state));

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

        let hints = connection_error_hints(HintSurface::Footer, &KeyContext::from_state(&state));

        assert_eq!(hints[0].short, "Edit");
        assert!(hints.iter().all(|hint| hint.short != "Close"));
    }
}
