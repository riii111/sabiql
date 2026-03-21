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
    let scroll_offset = state.explain.compare_scroll_offset;

    let mut lines: Vec<Line> = Vec::new();

    if let (Some(l), Some(r)) = (left, right) {
        render_verdict_section(&mut lines, l, r, area.width);
    }

    if area.width >= 60 {
        render_slot_columns(&mut lines, left, right, area.width);
    } else {
        render_slot_stacked(&mut lines, left, right);
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " Run EXPLAIN (Ctrl+E) to start comparing.",
            Style::default().fg(Theme::PLACEHOLDER_TEXT),
        )));
    }

    let max_scroll = lines.len().saturating_sub(area.height as usize);
    let clamped = scroll_offset.min(max_scroll);
    let visible: Vec<Line> = lines.into_iter().skip(clamped).collect();
    frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), area);
}

// ── Verdict (only when both slots are populated) ─────────────────────────────

fn render_verdict_section(
    lines: &mut Vec<Line>,
    left: &CompareSlot,
    right: &CompareSlot,
    width: u16,
) {
    let result = explain_plan::compare_plans(&left.plan, &right.plan);

    let (verdict_label, verdict_style) = match result.verdict {
        ComparisonVerdict::Improved => (
            "\u{2193} Improved",
            Style::default()
                .fg(Theme::STATUS_SUCCESS)
                .add_modifier(Modifier::BOLD),
        ),
        ComparisonVerdict::Worsened => (
            "\u{2191} Worsened",
            Style::default()
                .fg(Theme::STATUS_ERROR)
                .add_modifier(Modifier::BOLD),
        ),
        ComparisonVerdict::Similar => (
            "\u{2248} Similar",
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        ComparisonVerdict::Unavailable => (
            "Comparison unavailable",
            Style::default()
                .fg(Theme::TEXT_MUTED)
                .add_modifier(Modifier::BOLD),
        ),
    };

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

    let sep = "\u{2500}".repeat(width.saturating_sub(2) as usize);
    lines.push(Line::styled(
        format!(" {}", sep),
        Style::default().fg(Theme::MODAL_BORDER),
    ));
    lines.push(Line::raw(""));
}

// ── Side-by-side slot columns (shared across all states) ─────────────────────

fn render_slot_columns(
    lines: &mut Vec<Line>,
    left: Option<&CompareSlot>,
    right: Option<&CompareSlot>,
    total_width: u16,
) {
    let half = (total_width.saturating_sub(3) / 2) as usize;
    let sep = Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER));

    let active_header = Style::default()
        .fg(Theme::TEXT_ACCENT)
        .add_modifier(Modifier::BOLD);
    let empty_header = Style::default()
        .fg(Theme::TEXT_DIM)
        .add_modifier(Modifier::BOLD);

    let left_label = match left {
        Some(s) => format!(" Left [{}]", source_badge(&s.source)),
        None => " Left".to_string(),
    };
    let right_label = match right {
        Some(s) => format!("Right [{}]", source_badge(&s.source)),
        None => "Right".to_string(),
    };

    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(&left_label, half),
            if left.is_some() {
                active_header
            } else {
                empty_header
            },
        ),
        sep.clone(),
        Span::styled(
            pad_or_truncate(&right_label, half),
            if right.is_some() {
                active_header
            } else {
                empty_header
            },
        ),
    ]));

    let detail_style = Style::default().fg(Theme::TEXT_MUTED);
    let placeholder_style = Style::default().fg(Theme::PLACEHOLDER_TEXT);

    let left_detail = slot_detail_text(left);
    let right_detail = slot_detail_text(right);

    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(&left_detail, half),
            if left.is_some() {
                detail_style
            } else {
                placeholder_style
            },
        ),
        sep.clone(),
        Span::styled(
            pad_or_truncate(&right_detail, half),
            if right.is_some() {
                detail_style
            } else {
                placeholder_style
            },
        ),
    ]));

    lines.push(Line::raw(""));

    let l_plan: Vec<&str> = left
        .map(|s| s.plan.raw_text.lines().collect())
        .unwrap_or_default();
    let r_plan: Vec<&str> = right
        .map(|s| s.plan.raw_text.lines().collect())
        .unwrap_or_default();
    let max = l_plan.len().max(r_plan.len());

    for i in 0..max {
        let l = l_plan.get(i).unwrap_or(&"");
        let r = r_plan.get(i).unwrap_or(&"");

        let mut row_spans = vec![Span::raw(" ".to_string())];
        row_spans.extend(super::plan_highlight::highlight_truncated(
            l,
            half.saturating_sub(1),
        ));
        row_spans.push(sep.clone());
        row_spans.push(Span::raw(" ".to_string()));
        row_spans.extend(super::plan_highlight::highlight_truncated(
            r,
            half.saturating_sub(1),
        ));
        lines.push(Line::from(row_spans));
    }
}

// ── Stacked layout (narrow terminals) ────────────────────────────────────────

fn render_slot_stacked(
    lines: &mut Vec<Line>,
    left: Option<&CompareSlot>,
    right: Option<&CompareSlot>,
) {
    let header_style = Style::default()
        .fg(Theme::TEXT_ACCENT)
        .add_modifier(Modifier::BOLD);
    let empty_style = Style::default()
        .fg(Theme::TEXT_DIM)
        .add_modifier(Modifier::BOLD);
    let badge_style = Style::default().fg(Theme::TEXT_MUTED);

    render_stacked_slot(lines, "Left", left, header_style, empty_style, badge_style);
    lines.push(Line::raw(""));
    render_stacked_slot(
        lines,
        "Right",
        right,
        header_style,
        empty_style,
        badge_style,
    );
}

fn render_stacked_slot(
    lines: &mut Vec<Line>,
    label: &str,
    slot: Option<&CompareSlot>,
    active_style: Style,
    empty_style: Style,
    badge_style: Style,
) {
    match slot {
        Some(s) => {
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", label), active_style),
                Span::styled(format!("[{}]", source_badge(&s.source)), badge_style),
            ]));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(s.query_snippet.clone()),
                Span::styled(
                    format!("  ({})", mode_label(s.plan.is_analyze)),
                    badge_style,
                ),
            ]));
            for line in s.plan.raw_text.lines() {
                lines.push(super::plan_highlight::highlight_plan_line(line));
            }
        }
        None => {
            lines.push(Line::from(Span::styled(format!(" {}", label), empty_style)));
            lines.push(Line::from(Span::styled(
                "  Waiting...",
                Style::default().fg(Theme::PLACEHOLDER_TEXT),
            )));
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn slot_detail_text(slot: Option<&CompareSlot>) -> String {
    match slot {
        Some(s) => format!(" {}  ({})", s.query_snippet, mode_label(s.plan.is_analyze)),
        None => " Waiting...".to_string(),
    }
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

pub(super) fn pad_or_truncate(s: &str, width: usize) -> String {
    let char_count = s.chars().count();
    if char_count > width {
        s.chars().take(width.saturating_sub(1)).collect::<String>() + "\u{2026}"
    } else {
        format!("{:<width$}", s, width = width)
    }
}
