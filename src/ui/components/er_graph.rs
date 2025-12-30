use ratatui::Frame;
use ratatui::layout::{Layout, Constraint, Direction, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;

pub struct ErGraph;

impl ErGraph {
    pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let is_focused = state.focused_pane == FocusedPane::Graph;

        let title = match &state.er_graph {
            Some(graph) => {
                let related = graph.node_count().saturating_sub(1);
                format!(
                    " [1] Neighborhood [★{} + {} related, depth {}] ",
                    graph.center.split('.').last().unwrap_or(&graph.center),
                    related,
                    state.er_depth
                )
            }
            None => " [1] Neighborhood (select a table first) ".to_string(),
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
            let show_hint = graph.node_count() == 1;

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

            if show_hint {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(3), Constraint::Length(2)])
                    .split(block.inner(area));

                let list = List::new(items)
                    .highlight_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::REVERSED),
                    )
                    .highlight_symbol("> ");

                frame.render_widget(block, area);
                state.er_node_list_state.select(Some(state.er_selected_node));
                frame.render_stateful_widget(list, chunks[0], &mut state.er_node_list_state);

                let hint = if state.er_cache_sparse {
                    Paragraph::new(Line::from(vec![
                        Span::styled("⏳ ", Style::default().fg(Color::Yellow)),
                        Span::styled("FK metadata loading... ", Style::default().fg(Color::DarkGray)),
                        Span::styled("Tab", Style::default().fg(Color::Yellow)),
                        Span::styled(" to refresh", Style::default().fg(Color::DarkGray)),
                    ]))
                } else {
                    Paragraph::new(Line::from(vec![
                        Span::styled("No FK relations. ", Style::default().fg(Color::DarkGray)),
                        Span::styled(":erd!", Style::default().fg(Color::Yellow)),
                        Span::styled(" for full DB diagram", Style::default().fg(Color::DarkGray)),
                    ]))
                };
                frame.render_widget(hint, chunks[1]);
            } else {
                let list = List::new(items)
                    .block(block)
                    .highlight_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::REVERSED),
                    )
                    .highlight_symbol("> ");

                state.er_node_list_state.select(Some(state.er_selected_node));
                frame.render_stateful_widget(list, area, &mut state.er_node_list_state);
            }
        } else {
            let content =
                Paragraph::new("Switch to Browse tab and select a table, then return to ER tab.")
                    .block(block)
                    .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(content, area);
        }
    }
}
