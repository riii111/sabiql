use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::flash_timer::FlashId;
use crate::app::model::shared::text_input::TextInputState;
use crate::app::model::sql_editor::modal::{HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalStatus};
use crate::app::policy::write::sql_risk::AcknowledgeReason;
use crate::app::update::input::keybindings::sql_modal_plan_explain;
use crate::primitives::atoms::{apply_yank_flash, text_cursor_spans};
use crate::theme::ThemePalette;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    now: Instant,
    theme: &ThemePalette,
) -> u16 {
    if !state.session.active_db_capabilities().supports_explain() {
        let placeholder = Line::from(Span::styled(
            " EXPLAIN is unavailable for this database",
            Style::default().fg(theme.semantic.text.placeholder),
        ));
        frame.render_widget(Paragraph::new(vec![placeholder]), area);
        return area.height;
    }

    // Inline EXPLAIN ANALYZE confirmation for destructive DML
    if let SqlModalStatus::ConfirmingAnalyzeHigh {
        query,
        input,
        target_name,
    } = state.sql_modal.status()
    {
        let lines = build_analyze_confirm_lines(area, query, input, target_name, theme);
        render_scrolled(frame, area, lines, state.explain.confirm_scroll_offset());
        return area.height;
    }

    if let SqlModalStatus::ConfirmingAnalyzeRisk { query, reason } = state.sql_modal.status() {
        let lines = build_analyze_acknowledge_lines(area, query, reason, theme);
        render_scrolled(frame, area, lines, state.explain.confirm_scroll_offset());
        return area.height;
    }

    if let Some(error) = state.explain.error() {
        let lines: Vec<Line> = error
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.semantic.status.error),
                ))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        area.height
    } else if let Some(plan_text) = state.explain.plan_text() {
        let (label, label_style) = if state.explain.is_analyze() {
            (
                "EXPLAIN ANALYZE",
                Style::default()
                    .fg(theme.semantic.text.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                "EXPLAIN",
                Style::default()
                    .fg(theme.semantic.text.accent)
                    .add_modifier(Modifier::BOLD),
            )
        };
        let time_secs = state.explain.execution_time_ms() as f64 / 1000.0;
        let header = Line::from(vec![
            Span::styled(format!("{label} "), label_style),
            Span::styled(
                format!("({time_secs:.2}s)"),
                Style::default().fg(theme.semantic.text.muted),
            ),
        ]);

        let query_snippet = state.explain.plan_query_snippet().unwrap_or("");
        let query_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                query_snippet.to_string(),
                Style::default().fg(theme.semantic.text.muted),
            ),
        ]);

        let scroll = state.explain.scroll_offset();
        let mut lines = vec![header, query_line, Line::raw("")];
        lines.extend(
            plan_text
                .lines()
                .skip(scroll)
                .map(|line| super::plan_highlight::highlight_plan_line(line, theme)),
        );

        let flash_active = state.flash_timers.is_active(FlashId::SqlModal, now);
        let content_start = 3; // skip header, query snippet, empty line
        apply_yank_flash(&mut lines[content_start..], flash_active, theme);

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        area.height
    } else {
        let explain_key = sql_modal_plan_explain(state.settings.saved_keymap_preset()).key;
        let placeholder = Line::from(Span::styled(
            format!(" Press {explain_key} to run EXPLAIN"),
            Style::default().fg(theme.semantic.text.placeholder),
        ));
        frame.render_widget(Paragraph::new(vec![placeholder]), area);
        area.height
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
    query: &'a str,
    input: &'a TextInputState,
    name: &'a str,
    theme: &ThemePalette,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let header_style = Style::default()
        .fg(theme.semantic.status.error)
        .add_modifier(Modifier::BOLD);

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " \u{26a0} EXPLAIN ANALYZE",
        header_style,
    )));
    lines.push(Line::raw(""));

    let sep = "\u{2500}".repeat(area.width.saturating_sub(2) as usize);
    lines.push(Line::styled(format!(" {sep}"), theme.modal_border_style()));
    lines.push(Line::raw(""));

    lines.push(Line::from(Span::styled(
        " This is a destructive statement. EXPLAIN ANALYZE will",
        header_style,
    )));
    lines.push(Line::from(Span::styled(
        " execute it and data loss may occur.",
        header_style,
    )));
    lines.push(Line::raw(""));

    let full_query = query;
    for line in full_query.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {line}"),
            Style::default().fg(theme.semantic.text.dim),
        )));
    }
    lines.push(Line::raw(""));

    let is_match = input.content() == name;
    let prompt = format!(" Type \"{name}\" to confirm: > ");
    let mut prompt_spans = vec![Span::styled(
        prompt,
        Style::default().fg(theme.semantic.text.secondary),
    )];
    prompt_spans.extend(text_cursor_spans(
        input.content(),
        input.cursor(),
        input.viewport_offset(),
        HIGH_RISK_INPUT_VISIBLE_WIDTH,
        theme,
    ));
    if is_match {
        prompt_spans.push(Span::styled(
            " \u{2713}",
            Style::default().fg(theme.semantic.status.success),
        ));
    }
    lines.push(Line::from(prompt_spans));

    lines
}

fn build_analyze_acknowledge_lines<'a>(
    area: Rect,
    query: &'a str,
    reason: &AcknowledgeReason,
    theme: &ThemePalette,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    let (header_color, explanation) = match reason {
        AcknowledgeReason::UnknownRisk => (
            theme.semantic.status.warning,
            [
                " sabiql can't assess this statement's risk.",
                " EXPLAIN ANALYZE will execute it.",
            ],
        ),
        AcknowledgeReason::TargetNameUnavailable => (
            theme.semantic.status.error,
            [
                " This is a destructive statement. EXPLAIN ANALYZE will",
                " execute it and data loss may occur.",
            ],
        ),
    };
    let header_style = Style::default()
        .fg(header_color)
        .add_modifier(Modifier::BOLD);

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " \u{26a0} EXPLAIN ANALYZE",
        header_style,
    )));
    lines.push(Line::raw(""));

    let sep = "\u{2500}".repeat(area.width.saturating_sub(2) as usize);
    lines.push(Line::styled(format!(" {sep}"), theme.modal_border_style()));
    lines.push(Line::raw(""));

    for text in explanation {
        lines.push(Line::from(Span::styled(text, header_style)));
    }
    lines.push(Line::raw(""));

    for line in query.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {line}"),
            Style::default().fg(theme.semantic.text.dim),
        )));
    }
    lines.push(Line::raw(""));

    lines.push(Line::from(Span::styled(
        " Press Enter to execute  Esc: Cancel",
        Style::default().fg(theme.semantic.text.secondary),
    )));

    lines
}
