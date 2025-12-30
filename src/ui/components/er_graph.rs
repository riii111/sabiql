use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;

pub struct ErGraph;

impl ErGraph {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let is_focused = state.focused_pane == FocusedPane::Graph;

        let title = match &state.er_graph {
            Some(graph) => format!(
                " [1] Graph [{} tables, depth {}] ",
                graph.node_count(),
                state.er_depth
            ),
            None => " [1] Graph (select a table first) ".to_string(),
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

        if let Some(graph) = &state.er_graph {
            let items: Vec<ListItem> = graph
                .nodes
                .iter()
                .map(|node| {
                    let prefix = match node.hop_distance {
                        0 => "★ ",
                        1 => "├─ ",
                        _ => "│  ├─ ",
                    };

                    let text = format!("{}{}", prefix, node.qualified_name());

                    let style = if node.is_center() {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    ListItem::new(text).style(style)
                })
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::REVERSED),
                )
                .highlight_symbol("> ");

            state
                .er_node_list_state
                .select(Some(state.er_selected_node));
            frame.render_stateful_widget(list, area, &mut state.er_node_list_state);
        } else {
            let content =
                Paragraph::new("Switch to Browse tab and select a table, then return to ER tab.")
                    .block(block)
                    .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(content, area);
        }
    }
}
