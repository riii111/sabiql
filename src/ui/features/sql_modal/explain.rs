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

        let (banner_text, banner_style) = if *is_dml {
            (
                "\u{26a0} DML detected \u{2014} side effects will occur.",
                Style::default()
                    .fg(Theme::STATUS_ERROR)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                "EXPLAIN ANALYZE executes the query to collect actual statistics.",
                Style::default()
                    .fg(Theme::TEXT_ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
        };

        lines.push(Line::from(Span::styled(
            format!(" {}", banner_text),
            banner_style,
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                query_snippet.to_string(),
                Style::default().fg(Theme::TEXT_MUTED),
            ),
        ]));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Enter: confirm \u{2502} Esc: cancel",
            Style::default().fg(Theme::TEXT_DIM),
        )));

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
