pub(crate) mod command_palette;
pub(crate) mod er_table_picker;
pub(crate) mod query_history_picker;
pub(crate) mod table_picker;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::model::app_state::AppState;
use crate::app::update::input::keymap::{HintSurface, KeyContext, Picker, picker_hints};
use crate::primitives::molecules::{FooterHintBar, render_filter_input_line, render_modal};
use crate::theme::ThemePalette;

enum PickerModal {
    Table { count: usize },
    CommandPalette,
    Er { selected: usize, total: usize },
    QueryHistory { count: usize, height: u16 },
}

fn render_picker<T>(
    frame: &mut Frame,
    state: &AppState,
    theme: &ThemePalette,
    modal: PickerModal,
    layout: impl FnOnce(Rect) -> (Rect, T),
) -> (T, usize) {
    let (width, height, title, picker, prefix, filter, placeholder) = match modal {
        PickerModal::Table { count } => (
            60,
            Constraint::Percentage(70),
            " Table Picker ",
            Picker::Table,
            Some(format!("{count} tables")),
            state.ui.table_picker().filter_input(),
            None,
        ),
        PickerModal::CommandPalette => (
            50,
            Constraint::Percentage(50),
            " Command Palette ",
            Picker::CommandPalette,
            None,
            state.ui.table_picker().filter_input(),
            None,
        ),
        PickerModal::Er { selected, total } => (
            60,
            Constraint::Percentage(70),
            " ER Diagram ",
            Picker::Er,
            Some(format!("{selected}/{total} selected")),
            state.ui.er_picker().filter_input(),
            None,
        ),
        PickerModal::QueryHistory { count, height } => (
            70,
            Constraint::Max(height),
            " Query History ",
            Picker::QueryHistory,
            Some(format!("{count} entries │ type to filter")),
            state.query_history_picker.filter_input(),
            Some("type to filter"),
        ),
    };
    let hints = picker_hints(picker, HintSurface::Border, &KeyContext::from_state(state));
    let footer = match prefix {
        Some(prefix) => FooterHintBar::with_prefix(prefix, hints),
        None => FooterHintBar::new(hints),
    };
    let (_, inner) = render_modal(
        frame,
        Constraint::Percentage(width),
        height,
        title,
        footer,
        theme,
    );
    let (filter_area, content) = layout(inner);
    let width = render_filter_input_line(frame, filter_area, filter, placeholder, theme);
    (content, width)
}

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
