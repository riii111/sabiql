use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::explain_context::{CompareSlot, SlotSource};
use crate::app::state::AppState;
use crate::domain::explain_plan::{self, ComparisonVerdict};
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let left = state.explain.left.as_ref();
    let right = state.explain.right.as_ref();

    match (left, right) {
        (None, None) => {
            render_placeholder(frame, area, "Run EXPLAIN (Ctrl+E) to start comparing.");
        }
        (None, Some(right_slot)) => {
            render_single_slot(frame, area, "Right", right_slot, true);
        }
        (Some(left_slot), None) => {
            render_single_slot(frame, area, "Left", left_slot, false);
        }
        (Some(left_slot), Some(right_slot)) => {
            render_full_comparison(
                frame,
                area,
                left_slot,
                right_slot,
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
    frame.render_widget(Paragraph::new(vec![line]).wrap(Wrap { trim: false }), area);
}

fn source_badge(source: &SlotSource) -> &'static str {
    match source {
        SlotSource::AutoPrevious => "Previous",
        SlotSource::AutoLatest => "Latest",
        SlotSource::Manual => "Manual",
        SlotSource::Pinned => "Pinned",
    }
}

fn mode_label(is_analyze: bool) -> &'static str {
    if is_analyze { "ANALYZE" } else { "EXPLAIN" }
}

fn slot_header_line(label: &str, slot: &CompareSlot) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {} ", label),
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}]", source_badge(&slot.source)),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ])
}

fn slot_detail_line(slot: &CompareSlot) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::raw(slot.query_snippet.clone()),
        Span::styled(
            format!("  ({})", mode_label(slot.is_analyze)),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ])
}

fn render_single_slot(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    slot: &CompareSlot,
    prompt_left: bool,
) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(slot_header_line(label, slot));
    lines.push(slot_detail_line(slot));
    lines.push(Line::raw(""));

    let prompt = if prompt_left {
        "Run EXPLAIN on another query to compare."
    } else {
        "Run EXPLAIN (Ctrl+E) to populate the right slot."
    };
    lines.push(Line::from(Span::styled(
        format!(" {}", prompt),
        Style::default().fg(Theme::PLACEHOLDER_TEXT),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_full_comparison(
    frame: &mut Frame,
    area: Rect,
    left: &CompareSlot,
    right: &CompareSlot,
    scroll_offset: usize,
) {
    let result = explain_plan::compare_plans(&left.plan, &right.plan);

    let verdict_style = match result.verdict {
        ComparisonVerdict::Improved => Style::default()
            .fg(Theme::STATUS_SUCCESS)
            .add_modifier(Modifier::BOLD),
        ComparisonVerdict::Worsened => Style::default()
            .fg(Theme::STATUS_ERROR)
            .add_modifier(Modifier::BOLD),
        ComparisonVerdict::Similar => Style::default()
            .fg(Theme::TEXT_ACCENT)
            .add_modifier(Modifier::BOLD),
        ComparisonVerdict::Unavailable => Style::default()
            .fg(Theme::TEXT_MUTED)
            .add_modifier(Modifier::BOLD),
    };

    let verdict_label = match result.verdict {
        ComparisonVerdict::Improved => "\u{2193} Improved",
        ComparisonVerdict::Worsened => "\u{2191} Worsened",
        ComparisonVerdict::Similar => "\u{2248} Similar",
        ComparisonVerdict::Unavailable => "Comparison unavailable",
    };

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        format!(" {}", verdict_label),
        verdict_style,
    )));
    lines.push(Line::raw(""));

    for reason in &result.reasons {
        lines.push(Line::from(vec![
            Span::styled("  \u{2022} ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::raw(reason.clone()),
        ]));
    }

    if !result.reasons.is_empty() {
        lines.push(Line::raw(""));
    }

    let sep = "\u{2500}".repeat(area.width.saturating_sub(2) as usize);
    lines.push(Line::styled(
        format!(" {}", sep),
        Style::default().fg(Theme::MODAL_BORDER),
    ));
    lines.push(Line::raw(""));

    let use_side_by_side = area.width >= 60;

    if use_side_by_side {
        render_side_by_side(&mut lines, left, right, area.width);
    } else {
        render_stacked(&mut lines, left, right);
    }

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
    left: &CompareSlot,
    right: &CompareSlot,
    total_width: u16,
) {
    let half = (total_width.saturating_sub(3) / 2) as usize;

    // Slot headers
    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(&format!(" Left [{}]", source_badge(&left.source)), half),
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER)),
        Span::styled(
            pad_or_truncate(&format!("Right [{}]", source_badge(&right.source)), half),
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Query snippet + mode
    let left_detail = format!(" {} ({})", left.query_snippet, mode_label(left.is_analyze));
    let right_detail = format!("{} ({})", right.query_snippet, mode_label(right.is_analyze));
    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(&left_detail, half),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER)),
        Span::styled(
            pad_or_truncate(&right_detail, half),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ]));

    lines.push(Line::raw(""));

    // Plan text side-by-side
    let l_lines: Vec<&str> = left.plan.raw_text.lines().collect();
    let r_lines: Vec<&str> = right.plan.raw_text.lines().collect();
    let max_lines = l_lines.len().max(r_lines.len());

    for i in 0..max_lines {
        let l = l_lines.get(i).unwrap_or(&"");
        let r = r_lines.get(i).unwrap_or(&"");
        lines.push(Line::from(vec![
            Span::raw(pad_or_truncate(&format!(" {}", l), half)),
            Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER)),
            Span::raw(pad_or_truncate(r, half)),
        ]));
    }
}

fn render_stacked(lines: &mut Vec<Line>, left: &CompareSlot, right: &CompareSlot) {
    lines.push(slot_header_line("Left", left));
    lines.push(slot_detail_line(left));
    for line in left.plan.raw_text.lines() {
        lines.push(super::plan_highlight::highlight_plan_line(line));
    }
    lines.push(Line::raw(""));
    lines.push(slot_header_line("Right", right));
    lines.push(slot_detail_line(right));
    for line in right.plan.raw_text.lines() {
        lines.push(super::plan_highlight::highlight_plan_line(line));
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
