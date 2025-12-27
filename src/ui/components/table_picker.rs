use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::state::AppState;

use super::overlay::centered_rect;

pub struct TablePicker;

impl TablePicker {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(60),
            Constraint::Percentage(70),
        );

        // Clear the background
        frame.render_widget(Clear, area);

        // Outer block
        let block = Block::default()
            .title(" Table Picker (Ctrl+P) ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into filter input and list
        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(inner);

        // Filter input
        let filter_block = Block::default()
            .title(" Filter ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray));

        let filter_line = Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Yellow)),
            Span::raw(&state.filter_input),
            Span::styled("█", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ]);

        let filter_widget = Paragraph::new(filter_line).block(filter_block);
        frame.render_widget(filter_widget, filter_area);

        // Filtered tables list
        let filter_lower = state.filter_input.to_lowercase();
        let filtered: Vec<&String> = state
            .tables
            .iter()
            .filter(|t| t.to_lowercase().contains(&filter_lower))
            .collect();

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let style = if i == state.picker_selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(t.as_str()).style(style)
            })
            .collect();

        let list_block = Block::default()
            .title(format!(" Tables ({}) ", filtered.len()))
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray));

        let list = List::new(items).block(list_block);
        frame.render_widget(list, list_area);
    }
}
