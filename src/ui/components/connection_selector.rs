use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{List, ListItem};

use super::molecules::render_modal;
use super::scroll_indicator::{VerticalScrollParams, render_vertical_scroll_indicator_bar};
use crate::app::state::AppState;

pub struct ConnectionSelector;

impl ConnectionSelector {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let hint = " ↑/↓ Select  Enter Confirm  n New  q Quit ";
        let (_outer, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(60),
            " Select Connection ",
            hint,
        );

        Self::render_connection_list(frame, inner, state);
    }

    fn render_connection_list(
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        state: &mut AppState,
    ) {
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
                        Style::default().fg(Color::Green)
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
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut state.ui.connection_list_state);

        // Render vertical scrollbar if needed
        if !state.connections.is_empty() {
            let total_items = state.connections.len();
            let viewport_size = area.height as usize;

            if total_items > viewport_size {
                let scroll_offset = state.ui.connection_list_state.offset();

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
}
