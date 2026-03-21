use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::sql_modal_context::SqlModalStatus;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // Inline EXPLAIN ANALYZE confirmation banner
    if let SqlModalStatus::ConfirmingAnalyze { is_dml, .. } = state.sql_modal.status() {
        let query_snippet = state.sql_modal.content.lines().next().unwrap_or("");
        let mut lines = Vec::new();

        let warn_style = Style::default()
            .fg(Theme::STATUS_ERROR)
            .add_modifier(Modifier::BOLD);

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " \u{26a0} EXPLAIN ANALYZE",
            warn_style,
        )));
        lines.push(Line::raw(""));

        let sep = "\u{2500}".repeat(area.width.saturating_sub(2) as usize);
        lines.push(Line::styled(
            format!(" {}", sep),
            Style::default().fg(Theme::MODAL_BORDER),
        ));
        lines.push(Line::raw(""));

        if *is_dml {
            lines.push(Line::from(Span::styled(
                " This is a DML statement. EXPLAIN ANALYZE will execute it",
                warn_style,
            )));
            lines.push(Line::from(Span::styled(
                " and side effects (INSERT/UPDATE/DELETE) will occur.",
                warn_style,
            )));
        } else {
            lines.push(Line::from(Span::styled(
                " EXPLAIN ANALYZE will execute the query to collect actual",
                Style::default().fg(Theme::TEXT_PRIMARY),
            )));
            lines.push(Line::from(Span::styled(
                " runtime statistics.",
                Style::default().fg(Theme::TEXT_PRIMARY),
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  Query: ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(
                query_snippet.to_string(),
                Style::default().fg(Theme::TEXT_PRIMARY),
            ),
        ]));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(" {}", sep),
            Style::default().fg(Theme::MODAL_BORDER),
        ));

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }

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
        let (label, label_style) = if state.explain.is_analyze {
            (
                "EXPLAIN ANALYZE",
                Style::default()
                    .fg(Theme::STATUS_ERROR)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                "EXPLAIN",
                Style::default()
                    .fg(Theme::TEXT_ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
        };
        let time_secs = state.explain.execution_time_ms as f64 / 1000.0;
        let header = Line::from(vec![
            Span::styled(format!("{} ", label), label_style),
            Span::styled(
                format!("({:.2}s)", time_secs),
                Style::default().fg(Theme::TEXT_MUTED),
            ),
        ]);

        let query_snippet = state.sql_modal.content.lines().next().unwrap_or("");
        let query_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                query_snippet.to_string(),
                Style::default().fg(Theme::TEXT_MUTED),
            ),
        ]);

        let scroll = state.explain.scroll_offset;
        let mut lines = vec![header, query_line, Line::raw("")];
        lines.extend(
            plan_text
                .lines()
                .skip(scroll)
                .map(super::plan_highlight::highlight_plan_line),
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
