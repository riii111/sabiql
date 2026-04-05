use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::explain_context::CompareSlot;
use crate::domain::explain_plan::{self, ComparisonVerdict};
use crate::ui::theme::ThemePalette;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    now: Instant,
    theme: &ThemePalette,
) -> u16 {
    let can_yank = state.explain.left.is_some() && state.explain.right.is_some();
    let left = state.explain.left.as_ref();
    let right = state.explain.right.as_ref();
    let scroll_offset = state.explain.compare_scroll_offset;

    let mut lines: Vec<Line> = Vec::new();
    let mut flash_mask: Vec<bool> = Vec::new();

    if let (Some(l), Some(r)) = (left, right) {
        render_verdict_section(&mut lines, &mut flash_mask, l, r, area.width, theme);
    }

    if area.width >= 60 {
        render_slot_columns(&mut lines, &mut flash_mask, left, right, area.width, theme);
    } else {
        render_slot_stacked(&mut lines, &mut flash_mask, left, right, theme);
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " Run EXPLAIN (Ctrl+E) to start comparing.",
            Style::default().fg(theme.placeholder_text),
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
    crate::ui::primitives::atoms::apply_yank_flash_masked(
        &mut visible,
        flash_active,
        &visible_mask,
        theme,
    );

    frame.render_widget(
        Paragraph::new(visible)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false }),
        area,
    );
    area.height
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
    theme: &ThemePalette,
) {
    let result = explain_plan::compare_plans(&left.plan, &right.plan);

    let (verdict_label, verdict_style) = match result.verdict {
        ComparisonVerdict::Improved => (
            "\u{2193} Improved",
            Style::default()
                .fg(theme.status_success)
                .add_modifier(Modifier::BOLD),
        ),
        ComparisonVerdict::Worsened => (
            "\u{2191} Worsened",
            Style::default()
                .fg(theme.status_error)
                .add_modifier(Modifier::BOLD),
        ),
        ComparisonVerdict::Similar => (
            "\u{2248} Similar",
            Style::default()
                .fg(theme.text_accent)
                .add_modifier(Modifier::BOLD),
        ),
        ComparisonVerdict::Unavailable => (
            "Comparison unavailable",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        ),
    };

    push_empty(lines, flash_mask);
    push_content(
        lines,
        flash_mask,
        Line::from(Span::styled(format!(" {verdict_label}"), verdict_style)),
    );
    push_empty(lines, flash_mask);

    for reason in &result.reasons {
        push_content(
            lines,
            flash_mask,
            Line::from(vec![
                Span::styled("  \u{2022} ", Style::default().fg(theme.text_muted)),
                Span::styled(reason.clone(), Style::default().fg(theme.text_primary)),
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
        Line::styled(format!(" {sep}"), Style::default().fg(theme.modal_border)),
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
    theme: &ThemePalette,
) {
    let half = (total_width.saturating_sub(3) / 2) as usize;
    let sep = Span::styled(" \u{2502} ", Style::default().fg(theme.modal_border));

    let active_header = Style::default()
        .fg(theme.text_accent)
        .add_modifier(Modifier::BOLD);
    let empty_header = Style::default()
        .fg(theme.text_dim)
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

    let detail_style = Style::default().fg(theme.text_muted);
    let placeholder_style = Style::default().fg(theme.placeholder_text);

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
            Span::styled(format!(" {thin_sep}"), Style::default().fg(theme.text_dim)),
            sep.clone(),
            Span::styled(format!(" {thin_sep}"), Style::default().fg(theme.text_dim)),
        ]),
    );

    let dim_style = Style::default().fg(theme.text_dim);

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
            theme,
        ));
        row_spans.push(sep.clone());
        row_spans.push(Span::styled(" ".to_string(), dim_style));
        row_spans.extend(super::plan_highlight::highlight_truncated(
            r,
            half.saturating_sub(1),
            theme,
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
    theme: &ThemePalette,
) {
    let header_style = Style::default()
        .fg(theme.text_accent)
        .add_modifier(Modifier::BOLD);
    let badge_style = Style::default().fg(theme.text_muted);

    render_stacked_slot(
        lines,
        flash_mask,
        left,
        " Previous",
        header_style,
        badge_style,
        theme,
    );
    push_empty(lines, flash_mask);
    render_stacked_slot(
        lines,
        flash_mask,
        right,
        " Latest",
        header_style,
        badge_style,
        theme,
    );
}

fn render_stacked_slot(
    lines: &mut Vec<Line>,
    flash_mask: &mut Vec<bool>,
    slot: Option<&CompareSlot>,
    empty_label: &str,
    active_style: Style,
    badge_style: Style,
    theme: &ThemePalette,
) {
    if let Some(s) = slot {
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
                super::plan_highlight::highlight_plan_line(line, theme),
            );
        }
    } else {
        push_chrome(
            lines,
            flash_mask,
            Line::from(Span::styled(
                empty_label.to_string(),
                Style::default()
                    .fg(theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            )),
        );
        push_chrome(
            lines,
            flash_mask,
            Line::from(Span::styled(
                "  Run EXPLAIN again to compare",
                Style::default().fg(theme.placeholder_text),
            )),
        );
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
        format!("{s:<width$}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::explain_context::SlotSource;
    use crate::domain::explain_plan::ExplainPlan;
    use crate::ui::theme::DEFAULT_THEME;

    fn sample_slot(label: SlotSource, plan: &str) -> CompareSlot {
        CompareSlot {
            plan: ExplainPlan {
                raw_text: plan.to_string(),
                top_node_type: Some("Seq Scan".to_string()),
                total_cost: Some(10.0),
                estimated_rows: Some(1),
                is_analyze: false,
                execution_time_ms: 250,
            },
            query_snippet: "SELECT 1".to_string(),
            full_query: "SELECT 1".to_string(),
            source: label,
        }
    }

    #[test]
    fn stacked_compare_flashes_only_plan_content_rows() {
        let left = sample_slot(
            SlotSource::AutoPrevious,
            "Seq Scan on users  (cost=0.00..10.00 rows=1 width=32)\n  Filter: (id > 1)",
        );
        let right = sample_slot(
            SlotSource::AutoLatest,
            "Index Scan using users_pkey on users  (cost=0.00..5.00 rows=1 width=32)",
        );

        let mut lines = Vec::new();
        let mut flash_mask = Vec::new();
        render_slot_stacked(
            &mut lines,
            &mut flash_mask,
            Some(&left),
            Some(&right),
            &DEFAULT_THEME,
        );

        let rendered: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();

        assert_eq!(lines.len(), flash_mask.len());
        assert_eq!(
            flash_mask,
            vec![false, false, true, true, false, false, false, true]
        );
        assert!(rendered[0].contains("Previous"));
        assert!(rendered[1].contains("EXPLAIN"));
        assert!(rendered[2].contains("Seq Scan on users"));
        assert!(rendered[3].contains("Filter:"));
        assert!(rendered[4].is_empty());
        assert!(rendered[5].contains("Latest"));
        assert!(rendered[6].contains("EXPLAIN"));
        assert!(rendered[7].contains("Index Scan using users_pkey"));
    }

    #[test]
    fn stacked_compare_empty_slot_never_marks_flashable_rows() {
        let mut lines = Vec::new();
        let mut flash_mask = Vec::new();
        render_slot_stacked(&mut lines, &mut flash_mask, None, None, &DEFAULT_THEME);

        assert_eq!(lines.len(), flash_mask.len());
        assert!(flash_mask.iter().all(|&flash| !flash));
    }
}
