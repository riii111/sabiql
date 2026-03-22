use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::domain::MetadataState;

pub struct Header;

impl Header {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let db_name = state.session.database_name().unwrap_or("-");
        let table = state.session.selected_table_key().unwrap_or("-");

        let (status_text, status_color) = if state.session.dsn.is_none() {
            ("no dsn", Color::Red)
        } else {
            match &state.session.metadata_state() {
                MetadataState::Loaded => ("connected", Color::Green),
                MetadataState::Loading => ("loading...", Color::Yellow),
                MetadataState::Error(_) => ("error", Color::Red),
                MetadataState::NotLoaded => ("not loaded", Color::Gray),
            }
        };

        let connection_name = state
            .session
            .active_connection_name
            .as_deref()
            .unwrap_or("-");

        let mut line = Line::from(vec![
            Span::styled(
                &state.runtime.project_name,
                Style::default().fg(Color::Gray),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(db_name, Style::default().fg(Color::Gray)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(table, Style::default().fg(Color::Yellow)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(status_text, Style::default().fg(status_color)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(connection_name, Style::default().fg(Color::Gray)),
        ]);
        if state.session.read_only {
            line.push_span(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            line.push_span(Span::styled(
                "READ-ONLY",
                Style::default().fg(Color::Magenta),
            ));
        }

        frame.render_widget(Paragraph::new(line), area);
    }
}
