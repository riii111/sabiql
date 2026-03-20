use std::time::Instant;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::ui::theme::Theme;

use crate::app::state::AppState;
use crate::ui::primitives::atoms::key_chip;
use crate::ui::primitives::molecules::render_modal;

pub struct ConnectionError;

impl ConnectionError {
    pub fn render(frame: &mut Frame, state: &AppState) {
        Self::render_at(frame, state, Instant::now())
    }

    pub fn render_at(frame: &mut Frame, state: &AppState, now: Instant) {
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
            " Scroll: ↑/↓/j/k  Esc to close "
        } else {
            " Esc to close "
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

        Self::render_summary(frame, chunks[0], error_info.kind.summary());
        Self::render_hint(frame, chunks[2], state);
        Self::render_details_section(frame, chunks[4], error_state, details_expanded);
        Self::render_actions(frame, chunks[6], state, now);
    }

    fn render_summary(frame: &mut Frame, area: Rect, summary: &str) {
        let line = Line::from(vec![
            Span::styled("✗ ", Style::default().fg(Theme::STATUS_ERROR)),
            Span::styled(
                summary,
                Style::default()
                    .fg(Theme::STATUS_ERROR)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_hint(frame: &mut Frame, area: Rect, state: &AppState) {
        let hint = state
            .connection_error
            .error_info
            .as_ref()
            .map(|e| e.kind.hint())
            .unwrap_or("");
        let mut spans = vec![
            Span::styled("Hint: ", Style::default().fg(Theme::TEXT_ACCENT)),
            Span::styled(hint.to_string(), Style::default().fg(Theme::TEXT_SECONDARY)),
        ];
        if state.session.is_service_connection()
            && let Some(ref path) = state.runtime.service_file_path
        {
            spans.push(Span::styled(
                format!("  (edit {})", path.display()),
                Style::default().fg(Theme::TEXT_MUTED),
            ));
        }
        let line = Line::from(spans);
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
                    .fg(Theme::SECTION_HEADER)
                    .add_modifier(Modifier::BOLD),
            )]);
            frame.render_widget(Paragraph::new(toggle_line), chunks[0]);

            if let Some(details) = error_state.masked_details() {
                let lines: Vec<Line> = details
                    .lines()
                    .map(|l| Line::from(l.replace('\t', "    ")))
                    .collect();
                let scroll = error_state.scroll_offset;
                let para = Paragraph::new(lines)
                    .scroll((scroll as u16, 0))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Theme::TEXT_MUTED));
                frame.render_widget(para, chunks[1]);
            }
        } else {
            let toggle_line = Line::from(vec![
                Span::styled("▶ ", Style::default().fg(Theme::SECTION_HEADER)),
                Span::styled("Details ", Style::default().fg(Theme::SECTION_HEADER)),
                Span::styled(
                    "(press d to expand)",
                    Style::default().fg(Theme::TEXT_MUTED),
                ),
            ]);
            frame.render_widget(Paragraph::new(toggle_line), area);
        }
    }

    fn render_actions(frame: &mut Frame, area: Rect, state: &AppState, now: Instant) {
        let error_state = &state.connection_error;
        let mut spans = vec![Span::styled(
            "Actions: ",
            Style::default().fg(Theme::TEXT_MUTED),
        )];

        if state.session.is_service_connection() {
            spans.push(key_chip("r"));
            spans.push(Span::raw(" Retry  "));
        } else {
            spans.push(key_chip("e"));
            spans.push(Span::raw(" Re-enter  "));
        }

        spans.extend([
            key_chip("s"),
            Span::raw(" Switch  "),
            key_chip("d"),
            Span::raw(" Details  "),
            key_chip("y"),
            Span::raw(" Copy"),
        ]);

        if error_state.is_copied_visible_at(now) {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(
                "Copied!",
                Style::default()
                    .fg(Theme::STATUS_SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), area);
    }
}
