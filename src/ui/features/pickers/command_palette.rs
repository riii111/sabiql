use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem};

use crate::app::ui::model::app_state::AppState;
use crate::app::palette::palette_commands;
use crate::ui::theme::ThemePalette;

use crate::ui::primitives::molecules::render_modal;

pub struct CommandPalette;

impl CommandPalette {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(50),
            Constraint::Percentage(50),
            " Command Palette ",
            " Enter Select │ Esc Close ",
            theme,
        );

        let items: Vec<ListItem> = palette_commands()
            .enumerate()
            .map(|(i, entry)| {
                let style = if i == state.ui.table_picker.selected() {
                    theme.picker_selected_style()
                } else {
                    Style::default().fg(theme.semantic.text.secondary)
                };
                let content = format!("  {:<18} {}", entry.key, entry.description);
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }
}
