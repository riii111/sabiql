use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::AppState;

pub struct Header;

impl Header {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let db_name = state.database_name.as_deref().unwrap_or("-");
        let table = state.current_table.as_deref().unwrap_or("-");

        let line = Line::from(vec![
            Span::styled(&state.project_name, Style::default().fg(Color::Cyan)),
            Span::raw(" | "),
            Span::styled(&state.profile_name, Style::default().fg(Color::Yellow)),
            Span::raw(" | "),
            Span::raw(db_name),
            Span::raw(" | "),
            Span::raw(table),
            Span::raw(" | cache:--s | "),
            Span::styled("○ disconnected", Style::default().fg(Color::Red)),
        ]);

        frame.render_widget(Paragraph::new(line), area);
    }
}
