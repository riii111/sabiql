use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::catalog::{Hint, footer_hints, footer_status_text};
use crate::app::ui::AppServices;
use crate::app::ui::model::app_state::AppState;
use crate::app::ui::model::er_state::ErStatus;
use crate::ui::primitives::atoms::key_text;
use crate::ui::primitives::atoms::status_message::{MessageType, StatusMessage};
use crate::ui::theme::ThemePalette;

pub struct Footer;

impl Footer {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        services: &AppServices,
        time_ms: u128,
        theme: &ThemePalette,
    ) {
        let base_style = Style::default().fg(theme.semantic.text.primary);
        if state.er_preparation.status == ErStatus::Waiting {
            let line = Self::build_er_waiting_line(state, time_ms, theme);
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        } else if let Some(error) = &state.messages.last_error {
            let line = StatusMessage::render_line(error, MessageType::Error, theme);
            frame.render_widget(Paragraph::new(line).style(base_style), area);
        } else {
            // Show hints with optional inline success message
            let hints = footer_hints(state, services);
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
        time_ms: u128,
        theme: &ThemePalette,
    ) -> Line<'static> {
        let text = footer_status_text(state, time_ms)
            .expect("build_er_waiting_line is only used while ER preparation is waiting");
        Line::from(Span::styled(
            text,
            Style::default().fg(theme.semantic.text.accent),
        ))
    }

    fn build_hint_line_with_success(
        hints: &[Hint],
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

        for (i, hint) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(key_text(hint.key, theme));
            spans.push(Span::raw(format!(":{}", hint.description)));
        }

        Line::from(spans)
    }
}

#[cfg(test)]
mod tests {
    use crate::app::catalog::footer_hints;
    use crate::app::ui::AppServices;
    use crate::app::ui::model::app_state::AppState;
    use crate::app::ui::model::shared::db_capabilities::DbCapabilities;
    use crate::app::ui::model::shared::focused_pane::FocusedPane;
    use crate::app::ui::model::shared::input_mode::InputMode;
    use crate::app::ui::model::shared::inspector_tab::InspectorTab;
    use rstest::rstest;

    fn inspector_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);
        state.ui.focused_pane = FocusedPane::Inspector;
        state
    }

    #[rstest]
    #[case(DbCapabilities::new(true, vec![InspectorTab::Info]), false)]
    #[case(
        DbCapabilities::new(true, vec![InspectorTab::Info, InspectorTab::Columns]),
        true
    )]
    fn inspector_tabs_hint_visibility_tracks_supported_tab_count(
        #[case] db_capabilities: DbCapabilities,
        #[case] expected_visible: bool,
    ) {
        use crate::app::update::input::keybindings::{GLOBAL_KEYS, idx};

        let state = inspector_state();
        let mut services = AppServices::stub();
        services.db_capabilities = db_capabilities;

        let hints = footer_hints(&state, &services);
        let inspector_tabs_hint = {
            let (key, description) = GLOBAL_KEYS[idx::global::INSPECTOR_TABS].as_hint();
            super::Hint { key, description }
        };

        assert_eq!(hints.contains(&inspector_tabs_hint), expected_visible);
    }
}
