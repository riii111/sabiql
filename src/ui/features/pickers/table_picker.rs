use super::{PickerModal, render_picker, render_picker_list};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::ListItem;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::render_output::PickerLayout;
use crate::app::policy::table_kind::table_display_name;
use crate::primitives::atoms::Icon;
use crate::theme::ThemePalette;

pub(crate) struct TablePicker;

impl TablePicker {
    pub(crate) fn render(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> PickerLayout {
        let filtered_tables = state.filtered_tables();
        let filtered_count = filtered_tables.len();
        let (list_area, visible_width) = render_picker(
            frame,
            state,
            theme,
            PickerModal::Table {
                count: filtered_count,
            },
            |inner| {
                let areas: [_; 2] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
                areas.into()
            },
        );

        let items: Vec<ListItem> = filtered_tables
            .iter()
            .map(|t| {
                let content = format!(
                    "  {}{}",
                    Icon::from(t.kind_info.kind).prefix(state.settings.saved_icon_mode()),
                    table_display_name(
                        state.session.active_database_type_or_default(),
                        &t.schema,
                        &t.name,
                    )
                );
                ListItem::new(content).style(Style::default().fg(theme.semantic.text.secondary))
            })
            .collect();

        render_picker_list(
            frame,
            list_area,
            items,
            (filtered_count > 0).then_some(state.ui.table_picker().selected()),
            state.ui.table_picker().scroll_offset(),
            theme,
        );
        PickerLayout {
            pane_height: list_area.height,
            filter_visible_width: visible_width,
        }
    }
}
