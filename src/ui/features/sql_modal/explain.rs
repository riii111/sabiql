use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::sql_editor::modal::{HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalStatus};
use crate::ui::primitives::atoms::text_cursor_spans;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // Inline EXPLAIN ANALYZE confirmation
    match state.sql_modal.status() {
        SqlModalStatus::ConfirmingAnalyze { is_dml, .. } => {
            let lines = build_analyze_confirm_lines(area, state, *is_dml, None);
            render_scrolled(frame, area, lines, state.explain.confirm_scroll_offset);
            return;
        }
        SqlModalStatus::ConfirmingAnalyzeHigh {
            input, target_name, ..
        } => {
            let lines = build_analyze_confirm_lines(
                area,
                state,
                true,
                Some((input, target_name.as_deref())),
            );
            render_scrolled(frame, area, lines, state.explain.confirm_scroll_offset);
            return;
        }
        _ => {}
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
                    .fg(Theme::TEXT_ACCENT)
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

        let query_snippet = state.explain.plan_query_snippet.as_deref().unwrap_or("");
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

        let now = std::time::Instant::now();
        let flash_active = state.flash_timers.is_active(
            crate::app::model::shared::flash_timer::FlashId::SqlModal,
            now,
        );
        let content_start = 3; // skip header, query snippet, empty line
        crate::ui::primitives::atoms::apply_yank_flash(&mut lines[content_start..], flash_active);

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    } else {
        let placeholder = Line::from(Span::styled(
            " Press Ctrl+E to run EXPLAIN",
            Style::default().fg(Theme::PLACEHOLDER_TEXT),
        ));
        frame.render_widget(Paragraph::new(vec![placeholder]), area);
    }
}

fn render_scrolled(frame: &mut Frame, area: Rect, lines: Vec<Line>, scroll_offset: usize) {
    let max_scroll = lines.len().saturating_sub(area.height as usize);
    let clamped = scroll_offset.min(max_scroll);
    let visible: Vec<Line> = lines.into_iter().skip(clamped).collect();
    frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), area);
}

fn build_analyze_confirm_lines<'a>(
    area: Rect,
    state: &'a AppState,
    is_dml: bool,
    high_risk: Option<(
        &'a crate::app::model::shared::text_input::TextInputState,
        Option<&'a str>,
    )>,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let header_style = if is_dml {
        Style::default()
            .fg(Theme::STATUS_ERROR)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Theme::STATUS_WARNING)
            .add_modifier(Modifier::BOLD)
    };

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " \u{26a0} EXPLAIN ANALYZE",
        header_style,
    )));
    lines.push(Line::raw(""));

    let sep = "\u{2500}".repeat(area.width.saturating_sub(2) as usize);
    lines.push(Line::styled(
        format!(" {}", sep),
        Style::default().fg(Theme::MODAL_BORDER),
    ));
    lines.push(Line::raw(""));

    if let Some((input, target_name)) = high_risk {
        lines.push(Line::from(Span::styled(
            " This is a destructive statement. EXPLAIN ANALYZE will",
            header_style,
        )));
        lines.push(Line::from(Span::styled(
            " execute it and data loss may occur.",
            header_style,
        )));
        lines.push(Line::raw(""));

        let full_query = &state.sql_modal.content;
        for line in full_query.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Theme::TEXT_DIM),
            )));
        }
        lines.push(Line::raw(""));

        match target_name {
            Some(name) => {
                let is_match = input.content() == name;
                let prompt = format!(" Type \"{}\" to confirm: > ", name);
                let mut prompt_spans = vec![Span::styled(
                    prompt,
                    Style::default().fg(Theme::TEXT_SECONDARY),
                )];
                prompt_spans.extend(text_cursor_spans(
                    input.content(),
                    input.cursor(),
                    input.viewport_offset(),
                    HIGH_RISK_INPUT_VISIBLE_WIDTH,
                ));
                if is_match {
                    prompt_spans.push(Span::styled(
                        " \u{2713}",
                        Style::default().fg(Theme::STATUS_SUCCESS),
                    ));
                }
                lines.push(Line::from(prompt_spans));
            }
            None => {
                lines.push(Line::from(Span::styled(
                    " Cannot execute: unable to identify target table.  Esc: Back",
                    Style::default().fg(Theme::TEXT_MUTED),
                )));
            }
        }
    } else if is_dml {
        lines.push(Line::from(Span::styled(
            " This is a DML statement. EXPLAIN ANALYZE will execute it",
            header_style,
        )));
        lines.push(Line::from(Span::styled(
            " and side effects (INSERT/UPDATE/DELETE) will occur.",
            header_style,
        )));
        lines.push(Line::raw(""));

        let full_query = &state.sql_modal.content;
        for line in full_query.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Theme::TEXT_DIM),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            " EXPLAIN ANALYZE will execute the query to collect actual",
            Style::default().fg(Theme::TEXT_PRIMARY),
        )));
        lines.push(Line::from(Span::styled(
            " runtime statistics.",
            Style::default().fg(Theme::TEXT_PRIMARY),
        )));
        lines.push(Line::raw(""));

        let full_query = &state.sql_modal.content;
        for line in full_query.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Theme::TEXT_DIM),
            )));
        }
    }

    lines
}
