use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::render_output::PickerRenderMetrics;
use crate::primitives::molecules::{FooterHintBar, render_filter_input_line, render_modal};
use crate::theme::ThemePalette;

pub struct TablePicker;

impl TablePicker {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> PickerRenderMetrics {
        let filtered_count = state.filtered_tables().len();
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            " Table Picker ",
            FooterHintBar::with_prefix(format!("{filtered_count} tables"), [("Enter", "Select")]),
            theme,
        );

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        let visible_width = render_filter_input_line(
            frame,
            filter_area,
            state.ui.table_picker.filter_input(),
            None,
            theme,
        );

        let filtered = state.filtered_tables();
        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| {
                let content = format!("  {}", t.qualified_name());
                ListItem::new(content).style(Style::default().fg(theme.semantic.text.secondary))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme.picker_selected_style())
            .highlight_symbol("▸ ");

        let selected = if filtered_count > 0 {
            Some(state.ui.table_picker.selected())
        } else {
            None
        };
        let mut list_state = ListState::default()
            .with_selected(selected)
            .with_offset(state.ui.table_picker.scroll_offset());
        frame.render_stateful_widget(list, list_area, &mut list_state);
        PickerRenderMetrics {
            pane_height: list_area.height,
            filter_visible_width: visible_width,
        }
    }
}
