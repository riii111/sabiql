use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::AppState;
use crate::domain::MetadataState;

pub struct Header;

impl Header {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let db_name = state.runtime.database_name.as_deref().unwrap_or("-");
        let table = state.cache.current_table.as_deref().unwrap_or("-");

        let (status_text, status_color) = if state.runtime.dsn.is_none() {
            ("no dsn", Color::Red)
        } else {
            match &state.cache.state {
                MetadataState::Loaded => ("connected", Color::Green),
                MetadataState::Loading => ("loading...", Color::Yellow),
                MetadataState::Error(_) => ("error", Color::Red),
                MetadataState::NotLoaded => ("not loaded", Color::Gray),
            }
        };

        let connection_name = state
            .runtime
            .active_connection_name
            .as_deref()
            .unwrap_or("-");

        let line = Line::from(vec![
            Span::styled(
                &state.runtime.project_name,
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" | "),
            Span::styled(
                &state.runtime.profile_name,
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" | "),
            Span::raw(db_name),
            Span::raw(" | "),
            Span::raw(table),
            Span::raw(" | "),
            Span::styled(status_text, Style::default().fg(status_color)),
            Span::raw(" | "),
            Span::styled(connection_name, Style::default().fg(Color::Magenta)),
        ]);

        frame.render_widget(Paragraph::new(line), area);
    }
}
