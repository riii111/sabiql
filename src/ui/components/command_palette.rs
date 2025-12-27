use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::app::action::Action;
use crate::app::state::AppState;

use super::overlay::centered_rect;

pub struct CommandPalette;

impl CommandPalette {
    const COMMANDS: &'static [(&'static str, &'static str)] = &[
        ("q / :quit", "Quit application"),
        ("? / :help", "Show help"),
        (":sql", "Open SQL Modal (PR4)"),
        (":open-console", "Open Console (PR5)"),
        ("Ctrl+P", "Open Table Picker"),
        ("f", "Toggle Focus mode"),
        ("r", "Reload metadata (PR3)"),
    ];

    pub fn command_count() -> usize {
        Self::COMMANDS.len()
    }

    pub fn action_for_index(index: usize) -> Action {
        match index {
            0 => Action::Quit,
            1 => Action::OpenHelp,
            2 => Action::None, // :sql (PR4)
            3 => Action::None, // :open-console (PR5)
            4 => Action::OpenTablePicker,
            5 => Action::ToggleFocus,
            6 => Action::None, // :reload (PR3)
            _ => Action::None,
        }
    }

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
            .style(Style::default().bg(Color::DarkGray));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let items: Vec<ListItem> = Self::COMMANDS
            .iter()
            .enumerate()
            .map(|(i, (key, desc))| {
                let style = if i == state.picker_selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let content = format!("{:<20} {}", key, desc);
                ListItem::new(content).style(style)
            })
            .collect();

        let list_block = Block::default()
            .title(" Commands ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray));

        let list = List::new(items).block(list_block);
        frame.render_widget(list, inner);
    }
}
