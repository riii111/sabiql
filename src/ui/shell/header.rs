use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::domain::MetadataState;
use crate::ui::theme::Theme;

pub struct Header;

impl Header {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let db_name = state.session.database_name().unwrap_or("-");
        let table = state.session.selected_table_key().unwrap_or("-");

        let sep_style = Style::default().fg(Theme::TEXT_MUTED);
        let item_style = Style::default().fg(Theme::TEXT_SECONDARY);

        let (status_text, status_color) = if state.session.dsn.is_none() {
            ("no dsn", Theme::STATUS_ERROR)
        } else {
            match &state.session.metadata_state() {
                MetadataState::Loaded => ("connected", Theme::STATUS_SUCCESS),
                MetadataState::Loading => ("loading...", Theme::STATUS_WARNING),
                MetadataState::Error(_) => ("error", Theme::STATUS_ERROR),
                MetadataState::NotLoaded => ("not loaded", Theme::TEXT_MUTED),
            }
        };

        let connection_name = state
            .session
            .active_connection_name
            .as_deref()
            .unwrap_or("-");

        let mut line = Line::from(vec![
            Span::styled(&state.runtime.project_name, item_style),
            Span::styled(" | ", sep_style),
            Span::styled(db_name, item_style),
            Span::styled(" | ", sep_style),
            Span::styled(table, Style::default().fg(Theme::TEXT_PRIMARY)),
            Span::styled(" | ", sep_style),
            Span::styled(status_text, Style::default().fg(status_color)),
            Span::styled(" | ", sep_style),
            Span::styled(connection_name, item_style),
        ]);
        if state.session.read_only {
            line.push_span(Span::styled(" | ", sep_style));
            line.push_span(Span::styled(
                "READ-ONLY",
                Style::default().fg(Theme::STATUS_WARNING),
            ));
        }

        frame.render_widget(Paragraph::new(line), area);
    }
}
