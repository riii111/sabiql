use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::query_history_state::GroupedEntry;
use crate::app::state::AppState;
use crate::domain::query_history::QueryResultStatus;
use crate::ui::primitives::molecules::render_modal;
use crate::ui::theme::Theme;

const TIMESTAMP_WIDTH: usize = 18;
const STATUS_WIDTH: usize = 2;
const LIST_MIN_HEIGHT: u16 = 5;
const LIST_MAX_HEIGHT: u16 = 10;
const PREVIEW_MIN_HEIGHT: u16 = 6;
const MIN_INNER_FOR_PREVIEW: u16 = 10;

const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn format_short_timestamp(iso: &str) -> String {
    // "2026-03-17T00:48:52Z" -> "Mar 17 00:48 UTC"
    if iso.len() < 16 {
        return iso.to_string();
    }
    let month: usize = iso[5..7].parse().unwrap_or(0);
    let day = &iso[8..10];
    let time = &iso[11..16];
    let month_name = if (1..=12).contains(&month) {
        MONTH_ABBR[month - 1]
    } else {
        "???"
    };
    format!("{} {} {} UTC", month_name, day, time)
}

fn status_span(status: QueryResultStatus) -> Span<'static> {
    match status {
        QueryResultStatus::Success => {
            Span::styled("\u{2713} ", Style::default().fg(Theme::STATUS_SUCCESS))
        }
        QueryResultStatus::Failed => {
            Span::styled("\u{2717} ", Style::default().fg(Theme::STATUS_ERROR))
        }
    }
}

fn compute_preview_height(inner_height: u16) -> u16 {
    if inner_height < MIN_INNER_FOR_PREVIEW {
        return 0;
    }
    // filter takes 1 row
    let available = inner_height.saturating_sub(1);
    let desired = (inner_height * 30 / 100).max(PREVIEW_MIN_HEIGHT);
    let max_preview = available.saturating_sub(LIST_MIN_HEIGHT);
    desired.min(max_preview)
}

struct PreviewData {
    query: String,
    result_status: QueryResultStatus,
    affected_rows: Option<u64>,
    executed_at: String,
}

pub struct QueryHistoryPicker;

impl QueryHistoryPicker {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let filter_is_empty = state.query_history_picker.filter_input.content().is_empty();
        let filter_content = state
            .query_history_picker
            .filter_input
            .content()
            .to_string();
        let scroll_offset = state.query_history_picker.scroll_offset;
        let raw_selected = state.query_history_picker.selected;

        let grouped = state.query_history_picker.grouped_filtered_entries();
        let grouped_count = grouped.len();
        let selected_idx = if grouped_count == 0 {
            0
        } else {
            raw_selected.min(grouped_count - 1)
        };

        let max_height = (frame.area().height * 70 / 100).max(MIN_INNER_FOR_PREVIEW + 2);
        let preview_est = if grouped_count > 0 {
            PREVIEW_MIN_HEIGHT + 1 // +1 for border
        } else {
            0
        };
        // border(2) + filter(1) + actual entries + preview — capped at 70%
        let desired_height = (2 + 1 + (grouped_count as u16).max(1) + preview_est).min(max_height);

        let border_footer = format!(
            " {} entries \u{2502} type to filter \u{2502} Enter Select ",
            grouped_count,
        );

        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Max(desired_height),
            " Query History ",
            &border_footer,
        );

        let preview_h = compute_preview_height(inner.height);
        let show_preview = preview_h > 0;

        let areas = if show_preview {
            let [filter_area, list_area, preview_area] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Max(LIST_MAX_HEIGHT),
                Constraint::Min(preview_h),
            ])
            .areas(inner);
            (filter_area, list_area, Some(preview_area))
        } else {
            let [filter_area, list_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
            (filter_area, list_area, None)
        };
        let (filter_area, list_area, preview_area) = areas;

        let filter_line = if filter_content.is_empty() {
            Line::from(Span::styled(
                "  type to filter",
                Style::default().fg(Theme::PLACEHOLDER_TEXT),
            ))
        } else {
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(Theme::MODAL_TITLE)),
                Span::raw(filter_content),
                Span::styled(
                    "\u{2588}",
                    Style::default()
                        .fg(Theme::CURSOR_FG)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ])
        };
        frame.render_widget(Paragraph::new(filter_line), filter_area);

        if grouped_count == 0 {
            drop(grouped);
            state.query_history_picker.pane_height = list_area.height;
            let msg = if filter_is_empty {
                "No history yet"
            } else {
                "No matches"
            };
            let empty_line = Line::from(Span::styled(
                format!("  {}", msg),
                Style::default().fg(Theme::TEXT_SECONDARY),
            ));
            frame.render_widget(Paragraph::new(empty_line), list_area);
            if let Some(pa) = preview_area {
                render_empty_preview(frame, pa);
            }
            return;
        }

        let available_width = list_area.width as usize;
        let query_max = available_width.saturating_sub(STATUS_WIDTH + TIMESTAMP_WIDTH + 4);

        let preview_data = grouped.get(selected_idx).map(|ge| PreviewData {
            query: ge.entry.query.clone(),
            result_status: ge.entry.result_status,
            affected_rows: ge.entry.affected_rows,
            executed_at: ge.entry.executed_at.as_str().to_string(),
        });

        let items: Vec<ListItem> = grouped
            .iter()
            .enumerate()
            .map(|(i, ge)| build_list_item(ge, i, selected_idx, query_max))
            .collect();

        drop(grouped);
        state.query_history_picker.pane_height = list_area.height;

        if let Some(pa) = preview_area {
            if let Some(ref pd) = preview_data {
                render_preview(frame, pa, pd);
            } else {
                render_empty_preview(frame, pa);
            }
        }

        let highlight_style = Style::default()
            .bg(Theme::COMPLETION_SELECTED_BG)
            .fg(Theme::TEXT_PRIMARY)
            .add_modifier(Modifier::BOLD);

        let list = List::new(items)
            .highlight_style(highlight_style)
            .highlight_symbol("\u{25b8} ");

        let mut list_state = ListState::default()
            .with_selected(Some(selected_idx))
            .with_offset(scroll_offset);
        frame.render_stateful_widget(list, list_area, &mut list_state);
    }
}

fn build_list_item(
    ge: &GroupedEntry<'_>,
    i: usize,
    selected_idx: usize,
    query_max: usize,
) -> ListItem<'static> {
    let query_display = ge.entry.query.replace('\n', " ");
    let char_len = query_display.chars().count();
    let truncated = if char_len > query_max && query_max > 3 {
        let s: String = query_display.chars().take(query_max - 1).collect();
        format!("{}\u{2026}", s)
    } else {
        query_display
    };

    let ts_short = format_short_timestamp(ge.entry.executed_at.as_str());

    let mut spans = vec![status_span(ge.entry.result_status)];

    if ge.match_indices.is_empty() {
        spans.push(Span::styled(
            truncated.clone(),
            Style::default().fg(if i == selected_idx {
                Theme::TEXT_PRIMARY
            } else {
                Theme::TEXT_SECONDARY
            }),
        ));
    } else {
        let chars: Vec<char> = truncated.chars().collect();
        for (ci, ch) in chars.iter().enumerate() {
            let is_match = ge.match_indices.contains(&(ci as u32));
            let color = if is_match {
                Theme::TEXT_ACCENT
            } else if i == selected_idx {
                Theme::TEXT_PRIMARY
            } else {
                Theme::TEXT_SECONDARY
            };
            let mut style = Style::default().fg(color);
            if is_match {
                style = style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(ch.to_string(), style));
        }
    }

    let badge = if ge.count > 1 {
        format!(" (\u{00d7}{})", ge.count)
    } else {
        String::new()
    };

    // Pad query + badge to fixed width so timestamp column aligns
    let query_chars = truncated.chars().count();
    let badge_chars = badge.chars().count();
    let used = query_chars + badge_chars;
    let pad = query_max.saturating_sub(used);

    if !badge.is_empty() {
        spans.push(Span::styled(badge, Style::default().fg(Theme::TEXT_DIM)));
    }

    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(
        format!("  {ts_short}"),
        Style::default().fg(Theme::TEXT_DIM),
    ));

    ListItem::new(Line::from(spans))
}

fn render_preview(frame: &mut Frame, area: ratatui::layout::Rect, pd: &PreviewData) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Theme::MODAL_BORDER))
        .title(Span::styled(
            " Preview ",
            Style::default().fg(Theme::MODAL_TITLE),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    let mut meta_spans = Vec::new();
    match pd.result_status {
        QueryResultStatus::Success => {
            meta_spans.push(Span::styled(
                "\u{2713} Success",
                Style::default().fg(Theme::STATUS_SUCCESS),
            ));
        }
        QueryResultStatus::Failed => {
            meta_spans.push(Span::styled(
                "\u{2717} Failed",
                Style::default().fg(Theme::STATUS_ERROR),
            ));
        }
    }
    if let Some(rows) = pd.affected_rows {
        meta_spans.push(Span::styled(
            format!("  \u{2502} {} rows affected", rows),
            Style::default().fg(Theme::TEXT_SECONDARY),
        ));
    }
    meta_spans.push(Span::styled(
        format!("  \u{2502} {}", format_short_timestamp(&pd.executed_at)),
        Style::default().fg(Theme::TEXT_DIM),
    ));
    lines.push(Line::from(meta_spans));
    lines.push(Line::raw(""));

    for sql_line in pd.query.lines() {
        lines.push(Line::styled(
            sql_line.to_string(),
            Style::default().fg(Theme::TEXT_PRIMARY),
        ));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn render_empty_preview(frame: &mut Frame, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Theme::MODAL_BORDER))
        .title(Span::styled(
            " Preview ",
            Style::default().fg(Theme::MODAL_TITLE),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let msg = Paragraph::new(Line::styled(
        "No selection",
        Style::default().fg(Theme::TEXT_MUTED),
    ));
    frame.render_widget(msg, inner);
}
