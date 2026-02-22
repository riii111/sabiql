use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState};

use super::molecules::render_modal;
use super::scroll_indicator::{VerticalScrollParams, render_vertical_scroll_indicator_bar};
use crate::app::keybindings::{CONNECTION_SELECTOR_KEYS, idx};
use crate::app::state::AppState;
use crate::app::ui_state::list_scroll_offset;
use crate::ui::theme::Theme;

pub struct ConnectionSelector;

impl ConnectionSelector {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let hint = Self::build_hint_string();
        let (_outer, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(60),
            " Select Connection ",
            &hint,
        );

        Self::render_connection_list(frame, inner, state);
    }

    fn render_connection_list(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
        let active_id = state.runtime.active_connection_id.as_ref();

        let items: Vec<ListItem> = if state.connections.is_empty() {
            vec![ListItem::new(" No connections")]
        } else {
            state
                .connections
                .iter()
                .map(|conn| {
                    let is_active = active_id == Some(&conn.id);
                    let prefix = if is_active { "● " } else { "  " };
                    let text = format!("{}{}", prefix, conn.display_name());
                    let style = if is_active {
                        Style::default().fg(Theme::ACTIVE_INDICATOR)
                    } else {
                        Style::default()
                    };
                    ListItem::new(text).style(style)
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

        let mut list_state =
            ListState::default().with_selected(Some(state.ui.connection_list_selected));
        frame.render_stateful_widget(list, area, &mut list_state);

        // Render vertical scrollbar if needed
        if !state.connections.is_empty() {
            let total_items = state.connections.len();
            let viewport_size = area.height as usize;

            if total_items > viewport_size {
                let scroll_offset =
                    list_scroll_offset(state.ui.connection_list_selected, viewport_size);

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

    fn build_hint_string() -> String {
        let hints = [
            CONNECTION_SELECTOR_KEYS[idx::connection_selector::SELECT].as_hint(),
            CONNECTION_SELECTOR_KEYS[idx::connection_selector::CONFIRM].as_hint(),
            CONNECTION_SELECTOR_KEYS[idx::connection_selector::NEW].as_hint(),
            CONNECTION_SELECTOR_KEYS[idx::connection_selector::EDIT].as_hint(),
            CONNECTION_SELECTOR_KEYS[idx::connection_selector::DELETE].as_hint(),
            CONNECTION_SELECTOR_KEYS[idx::connection_selector::QUIT].as_hint(),
        ];
        let hint_parts: Vec<String> = hints
            .iter()
            .map(|(key, desc)| format!("{} {}", key, desc))
            .collect();
        format!(" {} ", hint_parts.join("  "))
    }
}
