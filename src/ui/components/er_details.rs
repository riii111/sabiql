use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::focused_pane::FocusedPane;
use crate::app::state::AppState;

pub struct ErDetails;

impl ErDetails {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let is_focused = state.focused_pane == FocusedPane::Details;

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" [2] Details ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let Some(graph) = &state.er_graph else {
            let content = Paragraph::new("(no graph)")
                .block(block)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(content, area);
            return;
        };

        let Some(node) = graph.nodes.get(state.er_selected_node) else {
            let content = Paragraph::new("(no node selected)")
                .block(block)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(content, area);
            return;
        };

        let qualified_name = node.qualified_name();
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Table: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&qualified_name),
            ]),
            Line::from(vec![
                Span::styled("Hop: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(
                    "{} ({})",
                    node.hop_distance,
                    if node.is_center() {
                        "center"
                    } else {
                        "neighbor"
                    }
                )),
            ]),
            Line::from(""),
        ];

        // Outgoing FKs (this table references other tables)
        let outgoing = graph.outgoing_edges(&qualified_name);
        if !outgoing.is_empty() {
            lines.push(Line::from(Span::styled(
                "References (outgoing FK):",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
            for edge in &outgoing {
                lines.push(Line::from(format!(
                    "  → {} ({} → {})",
                    edge.to_node,
                    edge.from_columns.join(", "),
                    edge.to_columns.join(", ")
                )));
                lines.push(Line::from(format!("    FK: {}", edge.fk_name)));
            }
            lines.push(Line::from(""));
        }

        // Incoming FKs (other tables reference this table)
        let incoming = graph.incoming_edges(&qualified_name);
        if !incoming.is_empty() {
            lines.push(Line::from(Span::styled(
                "Referenced by (incoming FK):",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )));
            for edge in &incoming {
                lines.push(Line::from(format!(
                    "  ← {} ({} → {})",
                    edge.from_node,
                    edge.from_columns.join(", "),
                    edge.to_columns.join(", ")
                )));
                lines.push(Line::from(format!("    FK: {}", edge.fk_name)));
            }
        }

        if outgoing.is_empty() && incoming.is_empty() {
            lines.push(Line::from(Span::styled(
                "(no FK relationships)",
                Style::default().fg(Color::DarkGray),
            )));
        }

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }
}
