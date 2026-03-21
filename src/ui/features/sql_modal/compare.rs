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

    let use_side_by_side = area.width >= 60;

    match (left, right) {
        (None, None) => {
            if use_side_by_side {
                render_empty_side_by_side(frame, area);
            } else {
                render_placeholder(frame, area, "Run EXPLAIN (Ctrl+E) to start comparing.");
            }
        }
        (None, Some(right_slot)) => {
            if use_side_by_side {
                render_partial_side_by_side(frame, area, None, Some(right_slot));
            } else {
                render_partial_stacked(frame, area, None, Some(right_slot));
            }
        }
        (Some(left_slot), None) => {
            if use_side_by_side {
                render_partial_side_by_side(frame, area, Some(left_slot), None);
            } else {
                render_partial_stacked(frame, area, Some(left_slot), None);
            }
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

fn render_empty_side_by_side(frame: &mut Frame, area: Rect) {
    let half = (area.width.saturating_sub(3) / 2) as usize;
    let separator = Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER));
    let header_style = Style::default()
        .fg(Theme::TEXT_DIM)
        .add_modifier(Modifier::BOLD);
    let placeholder_style = Style::default().fg(Theme::PLACEHOLDER_TEXT);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(pad_or_truncate(" Left", half), header_style),
            separator.clone(),
            Span::styled(pad_or_truncate("Right", half), header_style),
        ]),
        Line::from(vec![
            Span::styled(
                pad_or_truncate(" Run EXPLAIN (Ctrl+E)", half),
                placeholder_style,
            ),
            separator.clone(),
            Span::styled(pad_or_truncate("", half), placeholder_style),
        ]),
    ];
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " Run EXPLAIN (Ctrl+E) to start comparing.",
        placeholder_style,
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_partial_side_by_side(
    frame: &mut Frame,
    area: Rect,
    left: Option<&CompareSlot>,
    right: Option<&CompareSlot>,
) {
    let half = (area.width.saturating_sub(3) / 2) as usize;
    let separator = Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER));
    let active_header = Style::default()
        .fg(Theme::TEXT_ACCENT)
        .add_modifier(Modifier::BOLD);
    let empty_header = Style::default()
        .fg(Theme::TEXT_DIM)
        .add_modifier(Modifier::BOLD);
    let detail_style = Style::default().fg(Theme::TEXT_MUTED);
    let placeholder_style = Style::default().fg(Theme::PLACEHOLDER_TEXT);

    let (left_header, right_header) = match (left, right) {
        (Some(l), _) => (
            Span::styled(
                pad_or_truncate(&format!(" Left [{}]", source_badge(&l.source)), half),
                active_header,
            ),
            Span::styled(pad_or_truncate("Right", half), empty_header),
        ),
        (_, Some(r)) => (
            Span::styled(pad_or_truncate(" Left", half), empty_header),
            Span::styled(
                pad_or_truncate(&format!("Right [{}]", source_badge(&r.source)), half),
                active_header,
            ),
        ),
        _ => unreachable!(),
    };

    let mut lines = vec![Line::from(vec![
        left_header,
        separator.clone(),
        right_header,
    ])];

    let left_detail = left
        .map(|l| format!(" {}  ({})", l.query_snippet, mode_label(l.is_analyze)))
        .unwrap_or_default();
    let right_detail = right
        .map(|r| format!("{}  ({})", r.query_snippet, mode_label(r.is_analyze)))
        .unwrap_or_default();

    let left_detail_style = if left.is_some() {
        detail_style
    } else {
        placeholder_style
    };
    let right_detail_style = if right.is_some() {
        detail_style
    } else {
        placeholder_style
    };

    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(
                if left.is_some() {
                    &left_detail
                } else {
                    " Waiting..."
                },
                half,
            ),
            left_detail_style,
        ),
        separator.clone(),
        Span::styled(
            pad_or_truncate(
                if right.is_some() {
                    &right_detail
                } else {
                    "Waiting..."
                },
                half,
            ),
            right_detail_style,
        ),
    ]));

    lines.push(Line::raw(""));

    let slot = left.or(right).unwrap();
    let plan_lines: Vec<&str> = slot.plan.raw_text.lines().collect();
    let is_left = left.is_some();

    for pl in &plan_lines {
        if is_left {
            lines.push(Line::from(vec![
                Span::raw(pad_or_truncate(&format!(" {}", pl), half)),
                separator.clone(),
                Span::raw(pad_or_truncate("", half)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw(pad_or_truncate("", half)),
                separator.clone(),
                Span::raw(pad_or_truncate(pl, half)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_partial_stacked(
    frame: &mut Frame,
    area: Rect,
    left: Option<&CompareSlot>,
    right: Option<&CompareSlot>,
) {
    let mut lines: Vec<Line> = Vec::new();
    let slot = left.or(right).unwrap();
    let label = if left.is_some() { "Left" } else { "Right" };

    lines.push(Line::from(vec![
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
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::raw(slot.query_snippet.clone()),
        Span::styled(
            format!("  ({})", mode_label(slot.is_analyze)),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ]));
    lines.push(Line::raw(""));
    for line in slot.plan.raw_text.lines() {
        lines.push(super::plan_highlight::highlight_plan_line(line));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " Run EXPLAIN on another query to compare.",
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
    let separator = Span::styled(" \u{2502} ", Style::default().fg(Theme::MODAL_BORDER));

    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(&format!(" Left [{}]", source_badge(&left.source)), half),
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        separator.clone(),
        Span::styled(
            pad_or_truncate(&format!("Right [{}]", source_badge(&right.source)), half),
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let left_detail = format!(" {}  ({})", left.query_snippet, mode_label(left.is_analyze));
    let right_detail = format!(
        "{}  ({})",
        right.query_snippet,
        mode_label(right.is_analyze)
    );
    lines.push(Line::from(vec![
        Span::styled(
            pad_or_truncate(&left_detail, half),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
        separator.clone(),
        Span::styled(
            pad_or_truncate(&right_detail, half),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ]));

    lines.push(Line::raw(""));

    let l_lines: Vec<&str> = left.plan.raw_text.lines().collect();
    let r_lines: Vec<&str> = right.plan.raw_text.lines().collect();
    let max_lines = l_lines.len().max(r_lines.len());

    for i in 0..max_lines {
        let l = l_lines.get(i).unwrap_or(&"");
        let r = r_lines.get(i).unwrap_or(&"");
        lines.push(Line::from(vec![
            Span::raw(pad_or_truncate(&format!(" {}", l), half)),
            separator.clone(),
            Span::raw(pad_or_truncate(r, half)),
        ]));
    }
}

fn render_stacked(lines: &mut Vec<Line>, left: &CompareSlot, right: &CompareSlot) {
    let header_style = Style::default()
        .fg(Theme::TEXT_ACCENT)
        .add_modifier(Modifier::BOLD);
    let badge_style = Style::default().fg(Theme::TEXT_MUTED);

    lines.push(Line::from(vec![
        Span::styled(" Left ", header_style),
        Span::styled(format!("[{}]", source_badge(&left.source)), badge_style),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::raw(left.query_snippet.clone()),
        Span::styled(format!("  ({})", mode_label(left.is_analyze)), badge_style),
    ]));
    for line in left.plan.raw_text.lines() {
        lines.push(super::plan_highlight::highlight_plan_line(line));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(" Right ", header_style),
        Span::styled(format!("[{}]", source_badge(&right.source)), badge_style),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::raw(right.query_snippet.clone()),
        Span::styled(format!("  ({})", mode_label(right.is_analyze)), badge_style),
    ]));
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
