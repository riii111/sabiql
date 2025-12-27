use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::app::palette::PALETTE_COMMANDS;
use crate::app::state::AppState;

use super::overlay::centered_rect;

pub struct CommandPalette;

impl CommandPalette {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        );

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Command Palette (Ctrl+K) ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let items: Vec<ListItem> = PALETTE_COMMANDS
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let style = if i == state.picker_selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let content = format!("{:<20} {}", cmd.key, cmd.description);
                ListItem::new(content).style(style)
            })
            .collect();

        let list_block = Block::default()
            .title(" Commands ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(0x1e, 0x1e, 0x2e)));

        let list = List::new(items).block(list_block);
        frame.render_widget(list, inner);
    }
}
