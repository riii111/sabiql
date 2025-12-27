use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::AppState;

pub struct Footer;

impl Footer {
    pub fn render(frame: &mut Frame, area: Rect, _state: &AppState) {
        let line = Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(":Quit  "),
            Span::styled("1", Style::default().fg(Color::Yellow)),
            Span::raw(":Browse  "),
            Span::styled("2", Style::default().fg(Color::Yellow)),
            Span::raw(":ER  "),
            Span::styled("f", Style::default().fg(Color::Yellow)),
            Span::raw(":Focus  "),
            Span::styled("?", Style::default().fg(Color::Yellow)),
            Span::raw(":Help"),
        ]);

        frame.render_widget(Paragraph::new(line), area);
    }
}
