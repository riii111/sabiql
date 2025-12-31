use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;
use crate::domain::MetadataState;

pub struct Explorer;

impl Explorer {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let has_cached_data = state.metadata.is_some() && !state.tables().is_empty();
        let is_focused = state.focused_pane == FocusedPane::Explorer;

        let title = match &state.metadata_state {
            MetadataState::Loading => " [1] Explorer [Loading...] ".to_string(),
            MetadataState::Error(_) if has_cached_data => {
                format!(" [1] Explorer [{} tables - Stale] ", state.tables().len())
            }
            MetadataState::Error(_) => " [1] Explorer [Error] ".to_string(),
            MetadataState::Loaded => {
                let count = state.tables().len();
                format!(" [1] Explorer [{} tables] ", count)
            }
            MetadataState::NotLoaded => " [1] Explorer ".to_string(),
        };

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let items: Vec<ListItem> = if has_cached_data {
            // Show existing tables (even during loading or after error)
            state
                .tables()
                .iter()
                .map(|t| {
                    let text = t.qualified_name();
                    ListItem::new(text)
                })
                .collect()
        } else {
            match &state.metadata_state {
                MetadataState::Loading => {
                    vec![ListItem::new("Loading metadata...")]
                }
                MetadataState::Error(e) => {
                    vec![ListItem::new(format!("Error: {}", e))]
                }
                MetadataState::Loaded => {
                    vec![ListItem::new("No tables found")]
                }
                MetadataState::NotLoaded => {
                    vec![ListItem::new("Press 'r' to load metadata")]
                }
            }
        };

        // Calculate inner area before moving block
        let inner = block.inner(area);

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        if has_cached_data {
            state
                .explorer_list_state
                .select(Some(state.explorer_selected));
        } else {
            state.explorer_list_state.select(None);
        }

        frame.render_stateful_widget(list, area, &mut state.explorer_list_state);

        // Render vertical scrollbar
        if has_cached_data {
            let total_items = state.tables().len();
            let viewport_size = inner.height as usize;

            if total_items > viewport_size {
                let scroll_offset = state.explorer_list_state.offset();

                use super::scroll_indicator::{
                    VerticalScrollParams, render_vertical_scroll_indicator_bar,
                };
                render_vertical_scroll_indicator_bar(
                    frame,
                    inner,
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
