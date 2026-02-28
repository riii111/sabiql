use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use super::molecules::render_modal;
use super::scroll_indicator::{VerticalScrollParams, render_vertical_scroll_indicator_bar};
use crate::app::connection_list::ConnectionListItem;
use crate::app::keybindings::{CONNECTION_SELECTOR_ROWS, idx};
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct ConnectionSelector;

impl ConnectionSelector {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let is_service_selected = crate::app::connection_list::is_service_selected(
            &state.connection_list_items,
            state.ui.connection_list_selected,
        );
        let hint = Self::build_hint_string(is_service_selected);
        let (_outer, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(60),
            " Select Connection ",
            &hint,
        );

        render_connection_list(frame, inner, state);
    }

    fn build_hint_string(is_service_selected: bool) -> String {
        let mut hints = vec![
            CONNECTION_SELECTOR_ROWS[idx::connection_selector::SELECT].as_hint(),
            CONNECTION_SELECTOR_ROWS[idx::connection_selector::CONFIRM].as_hint(),
            CONNECTION_SELECTOR_ROWS[idx::connection_selector::NEW].as_hint(),
        ];
        if !is_service_selected {
            hints.push(CONNECTION_SELECTOR_ROWS[idx::connection_selector::EDIT].as_hint());
            hints.push(CONNECTION_SELECTOR_ROWS[idx::connection_selector::DELETE].as_hint());
        }
        hints.push(CONNECTION_SELECTOR_ROWS[idx::connection_selector::QUIT].as_hint());

        let hint_parts: Vec<String> = hints
            .iter()
            .map(|(key, desc)| format!("{} {}", key, desc))
            .collect();
        format!(" {} ", hint_parts.join("  "))
    }
}

/// Shared rendering logic for connection list (used by ConnectionSelector and Explorer).
pub fn render_connection_list(frame: &mut Frame, area: Rect, state: &mut AppState) {
    state.ui.connection_list_pane_height = area.height;
    let active_id = state.runtime.active_connection_id.as_ref();

    // highlight_symbol "> " takes 2 columns
    let content_width = area.width.saturating_sub(2) as usize;
    let source_label = "from pg_service.conf";

    let items: Vec<ListItem> = if state.connection_list_items.is_empty() {
        vec![ListItem::new(" No connections")]
    } else {
        state
            .connection_list_items
            .iter()
            .map(|item| match item {
                ConnectionListItem::Profile(i) => {
                    let conn = &state.connections[*i];
                    let is_active = active_id == Some(&conn.id);
                    let prefix = if is_active { "● " } else { "  " };
                    let text = format!("{}{}", prefix, conn.display_name());
                    let style = if is_active {
                        Style::default().fg(Theme::ACTIVE_INDICATOR)
                    } else {
                        Style::default()
                    };
                    ListItem::new(text).style(style)
                }
                ConnectionListItem::Service(i) => {
                    let entry = &state.service_entries[*i];
                    let is_active = active_id == Some(&entry.connection_id());
                    let prefix = if is_active { "● " } else { "  " };
                    let label_col = content_width * 40 / 100;
                    let min_gap = 2;
                    let max_name_len =
                        content_width.saturating_sub(prefix.len() + min_gap + source_label.len());
                    let name = if entry.service_name.len() > max_name_len {
                        format!("{}…", &entry.service_name[..max_name_len.saturating_sub(1)])
                    } else {
                        entry.service_name.clone()
                    };
                    let name_part = format!("{}{}", prefix, name);
                    let gap = label_col.saturating_sub(name_part.len()).max(min_gap);
                    let name_style = if is_active {
                        Style::default().fg(Theme::ACTIVE_INDICATOR)
                    } else {
                        Style::default().fg(Theme::TEXT_SECONDARY)
                    };
                    let line = Line::from(vec![
                        Span::styled(name_part, name_style),
                        Span::raw(" ".repeat(gap)),
                        Span::styled(source_label, Style::default().fg(Theme::TEXT_MUTED)),
                    ]);
                    ListItem::new(line)
                }
            })
            .collect()
    };

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut list_state = ListState::default()
        .with_selected(Some(state.ui.connection_list_selected))
        .with_offset(state.ui.connection_list_scroll_offset);
    frame.render_stateful_widget(list, area, &mut list_state);

    if !state.connection_list_items.is_empty() {
        let total_items = state.connection_list_items.len();
        let viewport_size = area.height as usize;

        if total_items > viewport_size {
            let scroll_offset = state.ui.connection_list_scroll_offset;

            render_vertical_scroll_indicator_bar(
                frame,
                area,
                VerticalScrollParams {
                    position: scroll_offset,
                    viewport_size,
                    total_items,
                },
            );
        }
    }
}
