use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::state::AppState;

use super::overlay::centered_rect;

pub struct TablePicker;

impl TablePicker {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(60),
            Constraint::Percentage(70),
        );

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Table Picker (Ctrl+P) ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(inner);

        let filter_block = Block::default()
            .title(" Filter ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        let filter_line = Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Yellow)),
            Span::raw(&state.filter_input),
            Span::styled("█", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ]);

        let filter_widget = Paragraph::new(filter_line).block(filter_block);
        frame.render_widget(filter_widget, filter_area);

        let filtered = state.filtered_tables();

        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| ListItem::new(t.qualified_name()))
            .collect();

        let list_block = Block::default()
            .title(format!(" Tables ({}) ", filtered.len()))
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        let list = List::new(items)
            .block(list_block)
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        if !filtered.is_empty() {
            state.picker_list_state.select(Some(state.picker_selected));
        } else {
            state.picker_list_state.select(None);
        }

        frame.render_stateful_widget(list, list_area, &mut state.picker_list_state);
    }
}
