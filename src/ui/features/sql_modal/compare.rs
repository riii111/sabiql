use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::state::AppState;
use crate::domain::explain_plan::{self, ComparisonVerdict};
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let baseline = state.explain.baseline.as_ref();
    let current = state.explain.current_parsed.as_ref();

    match (baseline, current) {
        (None, _) => {
            render_placeholder(
                frame,
                area,
                "No baseline saved. Press b on [Plan] tab to save.",
            );
        }
        (Some(_), None) => {
            render_placeholder(frame, area, "Run EXPLAIN first (Ctrl+E), then compare.");
        }
        (Some(baseline), Some(current)) => {
            render_comparison(
                frame,
                area,
                baseline,
                current,
                state.explain.compare_scroll_offset,
            );
        }
    }
}

fn render_placeholder(frame: &mut Frame, area: Rect, message: &str) {
    let line = Line::from(Span::styled(
        format!(" {}", message),
        Style::default().fg(Theme::PLACEHOLDER_TEXT),
    ));
    frame.render_widget(Paragraph::new(vec![line]), area);
}

fn render_comparison(
    frame: &mut Frame,
    area: Rect,
    baseline: &crate::domain::explain_plan::ExplainPlan,
    current: &crate::domain::explain_plan::ExplainPlan,
    scroll_offset: usize,
) {
    let result = explain_plan::compare_plans(baseline, current);

    if result.verdict == ComparisonVerdict::Unavailable {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            " Comparison unavailable",
            Style::default()
                .fg(Theme::TEXT_MUTED)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        for reason in &result.reasons {
            lines.push(Line::from(vec![
                Span::styled("  \u{2022} ", Style::default().fg(Theme::TEXT_MUTED)),
                Span::raw(reason.clone()),
            ]));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }

    let verdict_style = match result.verdict {
        ComparisonVerdict::Improved => Style::default()
            .fg(Theme::STATUS_SUCCESS)
            .add_modifier(Modifier::BOLD),
        ComparisonVerdict::Worsened => Style::default()
            .fg(Theme::STATUS_ERROR)
            .add_modifier(Modifier::BOLD),
        ComparisonVerdict::Similar | ComparisonVerdict::Unavailable => Style::default()
            .fg(Theme::TEXT_ACCENT)
            .add_modifier(Modifier::BOLD),
    };

    let verdict_label = match result.verdict {
        ComparisonVerdict::Improved => "\u{2193} Improved",
        ComparisonVerdict::Worsened => "\u{2191} Worsened",
        ComparisonVerdict::Similar | ComparisonVerdict::Unavailable => "\u{2248} Similar",
    };

    let mut lines: Vec<Line> = Vec::new();

    // Verdict header
    lines.push(Line::from(Span::styled(
        format!(" {}", verdict_label),
        verdict_style,
    )));
    lines.push(Line::raw(""));

    // Reasons
    for reason in &result.reasons {
        lines.push(Line::from(vec![
            Span::styled("  \u{2022} ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::raw(reason.clone()),
        ]));
    }

    if !result.reasons.is_empty() {
        lines.push(Line::raw(""));
    }

    // Separator
    let sep = "\u{2500}".repeat(area.width.saturating_sub(2) as usize);
    lines.push(Line::styled(
        format!(" {}", sep),
        Style::default().fg(Theme::MODAL_BORDER),
    ));
    lines.push(Line::raw(""));

    // Plan text comparison
    let use_side_by_side = area.width >= 60;

    if use_side_by_side {
        render_side_by_side(&mut lines, baseline, current, area.width);
    } else {
        render_stacked(&mut lines, baseline, current);
    }

    // Clamp scroll offset to content bounds
    let max_scroll = lines.len().saturating_sub(1);
    let clamped_offset = scroll_offset.min(max_scroll);

    let visible_lines: Vec<Line> = lines.into_iter().skip(clamped_offset).collect();
    frame.render_widget(
        Paragraph::new(visible_lines).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_side_by_side(
    lines: &mut Vec<Line>,
    baseline: &crate::domain::explain_plan::ExplainPlan,
    current: &crate::domain::explain_plan::ExplainPlan,
    total_width: u16,
) {
    let half = (total_width.saturating_sub(3) / 2) as usize; // 3 = " | " separator

    // Headers
    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(" Baseline", half),
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER)),
        Span::styled(
            pad_or_truncate("Current", half),
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let b_lines: Vec<&str> = baseline.raw_text.lines().collect();
    let c_lines: Vec<&str> = current.raw_text.lines().collect();
    let max_lines = b_lines.len().max(c_lines.len());

    for i in 0..max_lines {
        let b = b_lines.get(i).unwrap_or(&"");
        let c = c_lines.get(i).unwrap_or(&"");
        lines.push(Line::from(vec![
            Span::raw(pad_or_truncate(&format!(" {}", b), half)),
            Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER)),
            Span::raw(pad_or_truncate(c, half)),
        ]));
    }
}

fn render_stacked(
    lines: &mut Vec<Line>,
    baseline: &crate::domain::explain_plan::ExplainPlan,
    current: &crate::domain::explain_plan::ExplainPlan,
) {
    lines.push(Line::from(Span::styled(
        " Baseline:",
        Style::default()
            .fg(Theme::TEXT_ACCENT)
            .add_modifier(Modifier::BOLD),
    )));
    for line in baseline.raw_text.lines() {
        lines.push(Line::raw(format!("  {}", line)));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " Current:",
        Style::default()
            .fg(Theme::TEXT_ACCENT)
            .add_modifier(Modifier::BOLD),
    )));
    for line in current.raw_text.lines() {
        lines.push(Line::raw(format!("  {}", line)));
    }
}

fn pad_or_truncate(s: &str, width: usize) -> String {
    let char_count = s.chars().count();
    if char_count > width {
        s.chars().take(width.saturating_sub(1)).collect::<String>() + "\u{2026}"
    } else {
        format!("{:<width$}", s, width = width)
    }
}
