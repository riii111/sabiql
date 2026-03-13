use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::state::AppState;
use crate::ui::primitives::molecules::render_modal;
use crate::ui::theme::Theme;

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

        // Single fuzzy match per frame (W1)
        let filtered = state.query_history_picker.filtered_entries();
        let filtered_count = filtered.len();
        let selected_idx = if filtered_count == 0 {
            0
        } else {
            raw_selected.min(filtered_count - 1)
        };

        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Percentage(70),
            " Query History ",
            &format!(
                " {} entries \u{2502} \u{2191}\u{2193} Navigate \u{2502} Enter Select ",
                filtered_count,
            ),
        );

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        // Filter input with cursor
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

        if filtered_count == 0 {
            drop(filtered);
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

        // Build list items from filtered entries
        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, fe)| {
                let query_display = fe.entry.query.replace('\n', " ");
                let char_len = query_display.chars().count();
                let truncated = if char_len > 120 {
                    let s: String = query_display.chars().take(117).collect();
                    format!("{}...", s)
                } else {
                    query_display
                };

                let timestamp = fe.entry.executed_at.as_str();
                // executed_at is always ASCII ISO-8601, so byte slice is safe
                let ts_short = if timestamp.len() >= 16 {
                    &timestamp[..16]
                } else {
                    timestamp
                };

                let mut spans = vec![Span::raw("  ")];

                if fe.match_indices.is_empty() {
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
                        let is_match = fe.match_indices.contains(&(ci as u32));
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

                spans.push(Span::styled(
                    format!("  {}", ts_short),
                    Style::default().fg(Theme::TEXT_MUTED),
                ));

                ListItem::new(Line::from(spans))
            })
            .collect();

        drop(filtered);
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
