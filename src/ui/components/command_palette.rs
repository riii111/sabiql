use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Clear, List, ListItem};

use crate::app::palette::PALETTE_COMMANDS;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::overlay::{centered_rect, modal_block_with_hint, render_scrim};

pub struct CommandPalette;

impl CommandPalette {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        );

        render_scrim(frame);
        frame.render_widget(Clear, area);

        let block = modal_block_with_hint(
            " Command Palette ".to_string(),
            " ↑↓ Navigate │ Enter Select │ Esc Close ".to_string(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let items: Vec<ListItem> = PALETTE_COMMANDS
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let style = if i == state.ui.picker_selected {
                    Style::default()
                        .bg(Theme::COMPLETION_SELECTED_BG)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let content = format!("  {:<18} {}", cmd.key, cmd.description);
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }
}
