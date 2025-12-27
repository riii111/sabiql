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
        let db_name = state.database_name.as_deref().unwrap_or("-");
        let table = state.current_table.as_deref().unwrap_or("-");
        let cache_age = state.cache_age_display();

        let (status_text, status_color) = match &state.metadata_state {
            MetadataState::Loaded => ("connected", Color::Green),
            MetadataState::Loading => ("loading...", Color::Yellow),
            MetadataState::Error(_) => ("error", Color::Red),
            MetadataState::NotLoaded => ("not loaded", Color::Gray),
        };

        let line = Line::from(vec![
            Span::styled(&state.project_name, Style::default().fg(Color::Cyan)),
            Span::raw(" | "),
            Span::styled(&state.profile_name, Style::default().fg(Color::Yellow)),
            Span::raw(" | "),
            Span::raw(db_name),
            Span::raw(" | "),
            Span::raw(table),
            Span::raw(" | cache:"),
            Span::raw(&cache_age),
            Span::raw(" | "),
            Span::styled(status_text, Style::default().fg(status_color)),
        ]);

        frame.render_widget(Paragraph::new(line), area);
    }
}
