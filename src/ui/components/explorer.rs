use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::state::AppState;
use crate::domain::MetadataState;

pub struct Explorer;

impl Explorer {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let has_cached_data = state.metadata.is_some() && !state.tables().is_empty();

        let title = match &state.metadata_state {
            MetadataState::Loading => " Explorer [Loading...] ".to_string(),
            MetadataState::Error(_) if has_cached_data => {
                // Show stale data count with error indicator
                format!(" Explorer [{} tables - Stale] ", state.tables().len())
            }
            MetadataState::Error(_) => " Explorer [Error] ".to_string(),
            MetadataState::Loaded => {
                let count = state.tables().len();
                format!(" Explorer [{} tables] ", count)
            }
            MetadataState::NotLoaded => " Explorer ".to_string(),
        };

        let block = Block::default().title(title).borders(Borders::ALL);

        let items: Vec<ListItem> = if has_cached_data {
            // Show existing tables (even during loading or after error)
            state
                .tables()
                .iter()
                .map(|t| {
                    let mut text = t.qualified_name();
                    if t.has_rls {
                        text.push_str(" [RLS]");
                    }
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

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = ListState::default();
        if has_cached_data {
            list_state.select(Some(state.explorer_selected));
        }

        frame.render_stateful_widget(list, area, &mut list_state);
    }
}
