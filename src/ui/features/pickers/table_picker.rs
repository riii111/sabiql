use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::render_output::PickerLayout;
use crate::primitives::molecules::{FooterHintBar, render_filter_input_line, render_modal};
use crate::theme::ThemePalette;

pub struct TablePicker;

impl TablePicker {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) -> PickerLayout {
        let database_picker = state.ui.database_picker();
        let filtered_tables = state.filtered_tables();
        let filtered_databases = state.filtered_databases();
        let filtered_count = if database_picker {
            filtered_databases.len()
        } else {
            filtered_tables.len()
        };
        let title = if database_picker {
            " Database Picker "
        } else {
            " Table Picker "
        };
        let item_label = if database_picker {
            "databases"
        } else {
            "tables"
        };
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            title,
            FooterHintBar::with_prefix(
                format!("{filtered_count} {item_label}"),
                [("Enter", "Select")],
            ),
            theme,
        );

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        let visible_width = render_filter_input_line(
            frame,
            filter_area,
            state.ui.table_picker().filter_input(),
            None,
            theme,
        );

        let items: Vec<ListItem> = if database_picker {
            filtered_databases
                .iter()
                .map(|database| {
                    ListItem::new(format!("  {database}"))
                        .style(Style::default().fg(theme.semantic.text.secondary))
                })
                .collect()
        } else {
            filtered_tables
                .iter()
                .map(|t| {
                    let content = format!("  {}", t.qualified_name());
                    ListItem::new(content).style(Style::default().fg(theme.semantic.text.secondary))
                })
                .collect()
        };

        let list = List::new(items)
            .highlight_style(theme.picker_selected_style())
            .highlight_symbol("▸ ");

        let selected = if filtered_count > 0 {
            Some(state.ui.table_picker().selected())
        } else {
            None
        };
        let mut list_state = ListState::default()
            .with_selected(selected)
            .with_offset(state.ui.table_picker().scroll_offset());
        frame.render_stateful_widget(list, list_area, &mut list_state);
        PickerLayout {
            pane_height: list_area.height,
            filter_visible_width: visible_width,
        }
    }
}
