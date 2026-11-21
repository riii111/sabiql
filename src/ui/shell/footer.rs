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
    use std::sync::Arc;

    use super::Footer;
    use crate::app::domain::connection::{
        ConnectionEndpoint, MySqlConnectionConfig, MySqlSslMode, PostgresConnectionConfig,
        PostgresEndpoint, SqliteConnectionConfig, SslMode,
    };
    use crate::app::domain::{ConnectionId, DatabaseType};
    use crate::app::model::app_state::AppState;
    use crate::app::model::connection::error::ConnectionErrorInfo;
    use crate::app::model::connection::setup::ConnectionField;
    use crate::app::model::shared::focused_pane::FocusedPane;
    use crate::app::model::shared::input_mode::InputMode;
    use crate::app::model::shared::ui_state::FocusMode;
    use crate::app::ports::outbound::DbOperationError;
    use crate::app::update::input::keymap::{HintSurface, KeyContext, connection_error_hints};
    use rstest::rstest;

    fn contains(hints: &[(String, &str)], expected: (&str, &str)) -> bool {
        hints
            .iter()
            .any(|(key, label)| (key.as_str(), *label) == expected)
    }

    fn test_endpoint(database_type: DatabaseType, label: &str) -> Arc<ConnectionEndpoint> {
        Arc::new(match database_type {
            DatabaseType::PostgreSQL => ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Settings(
                PostgresConnectionConfig::new(label, 5432, "app", "user", "", SslMode::Prefer),
            )),
            DatabaseType::SQLite => ConnectionEndpoint::SQLite(
                SqliteConnectionConfig::new(label).expect("test SQLite path is valid"),
            ),
            DatabaseType::MySQL => ConnectionEndpoint::MySQL(MySqlConnectionConfig::new(
                label,
                3306,
                None,
                "user",
                "",
                MySqlSslMode::Preferred,
            )),
        })
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
            state.session.activate_connection(
                &ConnectionId::new(),
                "database",
                &test_endpoint(database_type, "sqlite://test.db"),
            );
        }

        let hints = Footer::get_context_hints(&state);

        assert_eq!(contains(&hints, ("Tab/⇧Tab", "InsTabs")), expected_visible);
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
        state.session.activate_connection(
            &ConnectionId::new(),
            "mysql",
            &test_endpoint(
                DatabaseType::MySQL,
                "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
            ),
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
