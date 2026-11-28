pub(crate) mod command_palette;
pub(crate) mod er_table_picker;
pub(crate) mod query_history_picker;
pub(crate) mod table_picker;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{List, ListItem, ListState};

use crate::theme::ThemePalette;

fn render_picker_list(
    frame: &mut Frame,
    area: Rect,
    items: Vec<ListItem<'_>>,
    selected: Option<usize>,
    offset: usize,
    theme: &ThemePalette,
) {
    let list = List::new(items)
        .highlight_style(theme.picker_selected_style())
        .highlight_symbol("▸ ");
    let mut state = ListState::default()
        .with_selected(selected)
        .with_offset(offset);
    frame.render_stateful_widget(list, area, &mut state);
}
