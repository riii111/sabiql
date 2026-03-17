use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::state::AppState;
use crate::domain::query_history::{QueryResultStatus, SqlCategory, classify_sql};
use crate::ui::primitives::molecules::render_modal;
use crate::ui::theme::Theme;

const TIMESTAMP_WIDTH: usize = 18;
const STATUS_WIDTH: usize = 2;
const COLOR_BAR_WIDTH: usize = 2;

fn status_span(status: Option<QueryResultStatus>) -> Span<'static> {
    match status {
        Some(QueryResultStatus::Success) => {
            Span::styled("\u{2713} ", Style::default().fg(Theme::STATUS_SUCCESS))
        }
        Some(QueryResultStatus::Failed) => {
            Span::styled("\u{2717} ", Style::default().fg(Theme::STATUS_ERROR))
        }
        None => Span::raw("  "),
    }
}

fn category_color(cat: SqlCategory) -> ratatui::style::Color {
    match cat {
        SqlCategory::Select => Theme::SQL_SELECT,
        SqlCategory::Dml => Theme::SQL_DML,
        SqlCategory::Ddl => Theme::SQL_DDL,
        SqlCategory::Tcl => Theme::SQL_TCL,
        SqlCategory::Other => Theme::TEXT_MUTED,
    }
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

        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Percentage(70),
            " Query History ",
            &format!(
                " {} entries \u{2502} \u{2191}\u{2193} Navigate \u{2502} Enter Select ",
                grouped_count,
            ),
        );

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        let filter_line = Line::from(vec![
            Span::styled("  > ", Style::default().fg(Theme::MODAL_TITLE)),
            Span::raw(filter_content),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(Theme::CURSOR_FG)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]);
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
            return;
        }

        let available_width = list_area.width as usize;
        let prefix_width = STATUS_WIDTH + COLOR_BAR_WIDTH;
        let query_max = available_width.saturating_sub(prefix_width + TIMESTAMP_WIDTH + 4);

        let items: Vec<ListItem> = grouped
            .iter()
            .enumerate()
            .map(|(i, ge)| {
                let query_display = ge.entry.query.replace('\n', " ");
                let char_len = query_display.chars().count();
                let truncated = if char_len > query_max && query_max > 3 {
                    let s: String = query_display.chars().take(query_max - 1).collect();
                    format!("{}\u{2026}", s)
                } else {
                    query_display
                };

                let timestamp = ge.entry.executed_at.as_str();
                let ts_short = if timestamp.len() >= 16 {
                    &timestamp[..16]
                } else {
                    timestamp
                };

                let category = classify_sql(&ge.entry.query);
                let bar_color = category_color(category);

                let mut spans = vec![
                    status_span(ge.entry.result_status),
                    Span::styled("\u{2588} ", Style::default().fg(bar_color)),
                ];

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

                if ge.count > 1 {
                    spans.push(Span::styled(
                        format!(" (\u{00d7}{})", ge.count),
                        Style::default().fg(Theme::TEXT_MUTED),
                    ));
                }

                spans.push(Span::styled(
                    format!("  {}", ts_short),
                    Style::default().fg(Theme::TEXT_MUTED),
                ));

                ListItem::new(Line::from(spans))
            })
            .collect();

        drop(grouped);
        state.query_history_picker.pane_height = list_area.height;

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Theme::COMPLETION_SELECTED_BG)
                    .fg(Theme::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("\u{25b8} ");

        let mut list_state = ListState::default()
            .with_selected(Some(selected_idx))
            .with_offset(scroll_offset);
        frame.render_stateful_widget(list, list_area, &mut list_state);
    }
}
