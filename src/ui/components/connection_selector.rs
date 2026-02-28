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
use crate::domain::connection::ConnectionId;
use crate::ui::theme::Theme;

/// Display width of the active/inactive prefix ("● " or "  ").
/// Both are exactly 2 terminal columns; using a constant avoids
/// byte-length vs display-width mismatches for the multibyte "●".
const PREFIX_DISPLAY_WIDTH: usize = 2;

/// Percentage of content width allocated to the service name label column.
const SERVICE_LABEL_COL_PERCENT: usize = 40;

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
        let r = CONNECTION_SELECTOR_ROWS;
        use idx::connection_selector as cs;

        let mut hints = vec![
            r[cs::CONFIRM].as_hint(),
            r[cs::SELECT].as_hint(),
            r[cs::NEW].as_hint(),
        ];
        if !is_service_selected {
            hints.push(r[cs::EDIT].as_hint());
            hints.push(r[cs::DELETE].as_hint());
        }
        hints.push(r[cs::CLOSE].as_hint());

        let hint_parts: Vec<String> = hints
            .iter()
            .map(|(key, desc)| format!("{} {}", key, desc))
            .collect();
        format!(" {} ", hint_parts.join("  "))
    }
}

fn active_prefix(is_active: bool) -> &'static str {
    if is_active { "● " } else { "  " }
}

fn render_profile_item(
    id: &ConnectionId,
    display_name: &str,
    active_id: Option<&ConnectionId>,
) -> ListItem<'static> {
    let is_active = active_id == Some(id);
    let prefix = active_prefix(is_active);
    let text = format!("{}{}", prefix, display_name);
    let style = if is_active {
        Style::default().fg(Theme::ACTIVE_INDICATOR)
    } else {
        Style::default()
    };
    ListItem::new(text).style(style)
}

fn render_service_item(
    display_name: &str,
    service_id: ConnectionId,
    active_id: Option<&ConnectionId>,
    content_width: usize,
    source_label: &str,
) -> ListItem<'static> {
    let is_active = active_id == Some(&service_id);
    let prefix = active_prefix(is_active);
    let min_gap = 2;
    let max_name_len =
        content_width.saturating_sub(PREFIX_DISPLAY_WIDTH + min_gap + source_label.len());

    let name = if display_name.chars().count() > max_name_len {
        let truncated: String = display_name
            .chars()
            .take(max_name_len.saturating_sub(1))
            .collect();
        format!("{}…", truncated)
    } else {
        display_name.to_owned()
    };

    let label_col = content_width * SERVICE_LABEL_COL_PERCENT / 100;
    let name_display_width = PREFIX_DISPLAY_WIDTH + name.chars().count();
    let gap = label_col.saturating_sub(name_display_width).max(min_gap);
    let name_part = format!("{}{}", prefix, name);

    let name_style = if is_active {
        Style::default().fg(Theme::ACTIVE_INDICATOR)
    } else {
        Style::default().fg(Theme::TEXT_SECONDARY)
    };
    let line = Line::from(vec![
        Span::styled(name_part, name_style),
        Span::raw(" ".repeat(gap)),
        Span::styled(
            source_label.to_owned(),
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ]);
    ListItem::new(line)
}

/// Shared rendering logic for connection list (used by ConnectionSelector).
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
                    render_profile_item(&conn.id, conn.display_name(), active_id)
                }
                ConnectionListItem::Service(i) => {
                    let entry = &state.service_entries[*i];
                    render_service_item(
                        entry.display_name(),
                        entry.connection_id(),
                        active_id,
                        content_width,
                        source_label,
                    )
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
