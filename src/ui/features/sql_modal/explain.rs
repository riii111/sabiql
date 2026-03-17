use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    if let Some(ref error) = state.explain.error {
        let lines: Vec<Line> = error
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Theme::STATUS_ERROR),
                ))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    } else if let Some(ref plan_text) = state.explain.plan_text {
        let label = if state.explain.is_analyze {
            "EXPLAIN ANALYZE"
        } else {
            "EXPLAIN"
        };
        let time_secs = state.explain.execution_time_ms as f64 / 1000.0;
        let header = Line::from(vec![
            Span::styled(
                format!("{} ", label),
                Style::default()
                    .fg(Theme::TEXT_ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({:.2}s)", time_secs),
                Style::default().fg(Theme::TEXT_MUTED),
            ),
        ]);

        let scroll = state.explain.scroll_offset;
        let mut lines = vec![header, Line::raw("")];
        lines.extend(
            plan_text
                .lines()
                .skip(scroll)
                .map(|l| Line::raw(l.to_string())),
        );

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    } else {
        let placeholder = Line::from(Span::styled(
            " Press Ctrl+E to run EXPLAIN",
            Style::default().fg(Theme::PLACEHOLDER_TEXT),
        ));
        frame.render_widget(Paragraph::new(vec![placeholder]), area);
    }
}
