use std::time::Instant;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::atoms::{key_chip, spinner_char};
use super::molecules::render_modal;
use crate::app::state::AppState;

pub struct ConnectionError;

impl ConnectionError {
    pub fn render(frame: &mut Frame, state: &AppState) {
        Self::render_at(frame, state, Instant::now(), None)
    }

    pub fn render_at(frame: &mut Frame, state: &AppState, now: Instant, time_ms: Option<u128>) {
        let error_state = &state.connection_error;
        let Some(ref error_info) = error_state.error_info else {
            return;
        };

        let details_expanded = error_state.details_expanded;
        let height = if details_expanded {
            Constraint::Percentage(60)
        } else {
            Constraint::Length(12)
        };

        let hint_text = if details_expanded {
            " Scroll: ↑/↓/j/k  Esc to close  q to quit "
        } else {
            " Esc to close  q to quit "
        };
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            height,
            " Connection Error ",
            hint_text,
        );

        let chunks = Layout::vertical([
            Constraint::Length(1), // Summary
            Constraint::Length(1), // Empty
            Constraint::Length(1), // Hint
            Constraint::Length(1), // Empty
            Constraint::Min(1),    // Details area
            Constraint::Length(1), // Empty before actions
            Constraint::Length(1), // Actions
        ])
        .split(inner);

        if error_state.is_retrying {
            Self::render_retrying(frame, chunks[0], time_ms);
        } else {
            Self::render_summary(frame, chunks[0], error_info.kind.summary());
        }
        Self::render_hint(frame, chunks[2], error_info.kind.hint());
        Self::render_details_section(frame, chunks[4], error_state, details_expanded);
        Self::render_actions(frame, chunks[6], error_state, now);
    }

    fn render_summary(frame: &mut Frame, area: Rect, summary: &str) {
        let line = Line::from(vec![
            Span::styled("✗ ", Style::default().fg(Color::Red)),
            Span::styled(
                summary,
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_retrying(frame: &mut Frame, area: Rect, time_ms: Option<u128>) {
        let now_ms = time_ms.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        });
        let spinner = spinner_char(now_ms);
        let line = Line::from(vec![
            Span::styled(format!("{} ", spinner), Style::default().fg(Color::Yellow)),
            Span::styled(
                "Retrying connection...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_hint(frame: &mut Frame, area: Rect, hint: &str) {
        let line = Line::from(vec![
            Span::styled("Hint: ", Style::default().fg(Color::Yellow)),
            Span::styled(hint, Style::default().fg(Color::Gray)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_details_section(
        frame: &mut Frame,
        area: Rect,
        error_state: &crate::app::connection_error_state::ConnectionErrorState,
        expanded: bool,
    ) {
        if expanded {
            let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

            let toggle_line = Line::from(vec![Span::styled(
                "▼ Details",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]);
            frame.render_widget(Paragraph::new(toggle_line), chunks[0]);

            if let Some(details) = error_state.masked_details() {
                let lines: Vec<Line> = details.lines().map(|l| Line::from(l.to_string())).collect();
                let scroll = error_state.scroll_offset;
                let para = Paragraph::new(lines)
                    .scroll((scroll as u16, 0))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(para, chunks[1]);
            }
        } else {
            let toggle_line = Line::from(vec![
                Span::styled("▶ ", Style::default().fg(Color::Cyan)),
                Span::styled("Details ", Style::default().fg(Color::Cyan)),
                Span::styled("(press d to expand)", Style::default().fg(Color::DarkGray)),
            ]);
            frame.render_widget(Paragraph::new(toggle_line), area);
        }
    }

    fn render_actions(
        frame: &mut Frame,
        area: Rect,
        error_state: &crate::app::connection_error_state::ConnectionErrorState,
        now: Instant,
    ) {
        if error_state.is_retrying {
            let line = Line::from(vec![Span::styled(
                "Please wait...",
                Style::default().fg(Color::DarkGray),
            )]);
            frame.render_widget(Paragraph::new(line), area);
            return;
        }

        let mut spans = vec![
            Span::styled("Actions: ", Style::default().fg(Color::DarkGray)),
            key_chip("r"),
            Span::raw(" Retry  "),
            key_chip("e"),
            Span::raw(" Re-enter  "),
            key_chip("d"),
            Span::raw(" Details  "),
            key_chip("c"),
            Span::raw(" Copy  "),
            key_chip("q"),
            Span::raw(" Quit"),
        ];

        if error_state.is_copied_visible_at(now) {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(
                "Copied!",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), area);
    }
}
