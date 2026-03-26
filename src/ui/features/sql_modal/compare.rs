use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::explain_context::CompareSlot;
use crate::domain::explain_plan::{self, ComparisonVerdict};
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState, now: Instant) {
    state.explain.compare_viewport_height = Some(area.height);

    let can_yank = state.explain.left.is_some() && state.explain.right.is_some();
    let left = state.explain.left.as_ref();
    let right = state.explain.right.as_ref();
    let scroll_offset = state.explain.compare_scroll_offset;

    let mut lines: Vec<Line> = Vec::new();
    let mut flash_mask: Vec<bool> = Vec::new();

    if let (Some(l), Some(r)) = (left, right) {
        render_verdict_section(&mut lines, &mut flash_mask, l, r, area.width);
    }

    if area.width >= 60 {
        render_slot_columns(&mut lines, &mut flash_mask, left, right, area.width);
    } else {
        render_slot_stacked(&mut lines, &mut flash_mask, left, right);
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " Run EXPLAIN (Ctrl+E) to start comparing.",
            Style::default().fg(Theme::PLACEHOLDER_TEXT),
        )));
        flash_mask.push(false);
    }

    let max_scroll = lines.len().saturating_sub(area.height as usize);
    let clamped = scroll_offset.min(max_scroll);
    let mut visible: Vec<Line> = lines.into_iter().skip(clamped).collect();
    let visible_mask: Vec<bool> = flash_mask.into_iter().skip(clamped).collect();

    let flash_active = can_yank
        && state.flash_timers.is_active(
            crate::app::model::shared::flash_timer::FlashId::SqlModal,
            now,
        );
    if flash_active {
        let flash_style = Style::default()
            .fg(Theme::YANK_FLASH_FG)
            .bg(Theme::YANK_FLASH_BG);
        for (line, &is_target) in visible.iter_mut().zip(visible_mask.iter()) {
            if is_target {
                *line = std::mem::take(line).style(flash_style);
            }
        }
    }

    frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), area);
}

fn push_empty(lines: &mut Vec<Line>, flash_mask: &mut Vec<bool>) {
    lines.push(Line::raw(""));
    flash_mask.push(false);
}

// Copied to clipboard — flash on yank
fn push_content(lines: &mut Vec<Line>, flash_mask: &mut Vec<bool>, line: Line<'static>) {
    lines.push(line);
    flash_mask.push(true);
}

// UI chrome — never flash
fn push_chrome(lines: &mut Vec<Line>, flash_mask: &mut Vec<bool>, line: Line<'static>) {
    lines.push(line);
    flash_mask.push(false);
}

// ── Verdict (only when both slots are populated) ─────────────────────────────

fn render_verdict_section(
    lines: &mut Vec<Line>,
    flash_mask: &mut Vec<bool>,
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

    push_empty(lines, flash_mask);
    push_content(
        lines,
        flash_mask,
        Line::from(Span::styled(format!(" {}", verdict_label), verdict_style)),
    );
    push_empty(lines, flash_mask);

    for reason in &result.reasons {
        push_content(
            lines,
            flash_mask,
            Line::from(vec![
                Span::styled("  \u{2022} ", Style::default().fg(Theme::TEXT_MUTED)),
                Span::styled(reason.clone(), Style::default().fg(Theme::TEXT_PRIMARY)),
            ]),
        );
    }
    if !result.reasons.is_empty() {
        push_empty(lines, flash_mask);
    }

    let sep = "\u{2500}".repeat(width.saturating_sub(2) as usize);
    push_chrome(
        lines,
        flash_mask,
        Line::styled(
            format!(" {}", sep),
            Style::default().fg(Theme::MODAL_BORDER),
        ),
    );
    push_empty(lines, flash_mask);
}

// ── Side-by-side slot columns (shared across all states) ─────────────────────

fn render_slot_columns(
    lines: &mut Vec<Line>,
    flash_mask: &mut Vec<bool>,
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
        Some(s) => format!(" {}", s.source.label()),
        None => " Previous".to_string(),
    };
    let right_label = match right {
        Some(s) => format!(" {}", s.source.label()),
        None => " Latest".to_string(),
    };

    push_chrome(
        lines,
        flash_mask,
        Line::from(vec![
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
        ]),
    );

    let detail_style = Style::default().fg(Theme::TEXT_MUTED);
    let placeholder_style = Style::default().fg(Theme::PLACEHOLDER_TEXT);

    let left_detail = slot_detail_text(left);
    let right_detail = slot_detail_text(right);

    push_chrome(
        lines,
        flash_mask,
        Line::from(vec![
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
        ]),
    );

    let thin_sep = "\u{2500}".repeat(half.saturating_sub(1));
    push_chrome(
        lines,
        flash_mask,
        Line::from(vec![
            Span::styled(
                format!(" {}", thin_sep),
                Style::default().fg(Theme::TEXT_DIM),
            ),
            sep.clone(),
            Span::styled(
                format!(" {}", thin_sep),
                Style::default().fg(Theme::TEXT_DIM),
            ),
        ]),
    );

    let dim_style = Style::default().fg(Theme::TEXT_DIM);

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

        let mut row_spans = vec![Span::styled(" ".to_string(), dim_style)];
        row_spans.extend(super::plan_highlight::highlight_truncated(
            l,
            half.saturating_sub(1),
        ));
        row_spans.push(sep.clone());
        row_spans.push(Span::styled(" ".to_string(), dim_style));
        row_spans.extend(super::plan_highlight::highlight_truncated(
            r,
            half.saturating_sub(1),
        ));
        push_content(lines, flash_mask, Line::from(row_spans));
    }
}

// ── Stacked layout (narrow terminals) ────────────────────────────────────────

fn render_slot_stacked(
    lines: &mut Vec<Line>,
    flash_mask: &mut Vec<bool>,
    left: Option<&CompareSlot>,
    right: Option<&CompareSlot>,
) {
    let header_style = Style::default()
        .fg(Theme::TEXT_ACCENT)
        .add_modifier(Modifier::BOLD);
    let badge_style = Style::default().fg(Theme::TEXT_MUTED);

    render_stacked_slot(
        lines,
        flash_mask,
        left,
        " Previous",
        header_style,
        badge_style,
    );
    push_empty(lines, flash_mask);
    render_stacked_slot(
        lines,
        flash_mask,
        right,
        " Latest",
        header_style,
        badge_style,
    );
}

fn render_stacked_slot(
    lines: &mut Vec<Line>,
    flash_mask: &mut Vec<bool>,
    slot: Option<&CompareSlot>,
    empty_label: &str,
    active_style: Style,
    badge_style: Style,
) {
    match slot {
        Some(s) => {
            push_chrome(
                lines,
                flash_mask,
                Line::from(Span::styled(format!(" {}", s.source.label()), active_style)),
            );
            let time_secs = s.plan.execution_secs();
            push_chrome(
                lines,
                flash_mask,
                Line::from(Span::styled(
                    format!("  {}  ({:.2}s)", mode_label(s.plan.is_analyze), time_secs),
                    badge_style,
                )),
            );
            for line in s.plan.raw_text.lines() {
                push_content(
                    lines,
                    flash_mask,
                    super::plan_highlight::highlight_plan_line(line),
                );
            }
        }
        None => {
            push_chrome(
                lines,
                flash_mask,
                Line::from(Span::styled(
                    empty_label.to_string(),
                    Style::default()
                        .fg(Theme::TEXT_DIM)
                        .add_modifier(Modifier::BOLD),
                )),
            );
            push_chrome(
                lines,
                flash_mask,
                Line::from(Span::styled(
                    "  Run EXPLAIN again to compare",
                    Style::default().fg(Theme::PLACEHOLDER_TEXT),
                )),
            );
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn slot_detail_text(slot: Option<&CompareSlot>) -> String {
    match slot {
        Some(s) => {
            let time_secs = s.plan.execution_secs();
            format!(" {}  ({:.2}s)", mode_label(s.plan.is_analyze), time_secs)
        }
        None => " Run EXPLAIN again".to_string(),
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
