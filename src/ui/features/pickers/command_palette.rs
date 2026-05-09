use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem};

use crate::app::model::app_state::AppState;
use crate::app::update::input::palette::palette_commands;
use crate::theme::ThemePalette;

use crate::primitives::molecules::{modal_hint_line, render_modal_with_hint_line};

pub struct CommandPalette;

impl CommandPalette {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal_with_hint_line(
            frame,
            Constraint::Percentage(50),
            Constraint::Percentage(50),
            " Command Palette ",
            modal_hint_line(&[("Enter", "Select"), ("Esc", "Close")], theme),
            theme,
        );

        let items: Vec<ListItem> = palette_commands()
            .enumerate()
            .map(|(i, kb)| {
                let style = if i == state.ui.table_picker.selected() {
                    theme.picker_selected_style()
                } else {
                    Style::default().fg(theme.semantic.text.secondary)
                };
                let content = format!("  {:<18} {}", kb.key, kb.description);
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }
}
