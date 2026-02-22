use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem};

use crate::app::palette::palette_commands;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::molecules::render_modal;

pub struct CommandPalette;

impl CommandPalette {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(50),
            Constraint::Percentage(50),
            " Command Palette ",
            " j/k / ↑↓ Navigate │ Enter Select │ Esc Close ",
        );

        let items: Vec<ListItem> = palette_commands()
            .enumerate()
            .map(|(i, kb)| {
                let style = if i == state.ui.picker_selected {
                    Style::default()
                        .bg(Theme::COMPLETION_SELECTED_BG)
                        .fg(Theme::TEXT_PRIMARY)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Theme::TEXT_SECONDARY)
                };
                let content = format!("  {:<18} {}", kb.key, kb.description);
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }
}
