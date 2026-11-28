use super::render_picker_list;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::ListItem;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::render_output::PickerLayout;
use crate::app::policy::table_kind::table_display_name;
use crate::app::update::input::keymap::{HintSurface, KeyContext, Picker, picker_hints};
use crate::primitives::atoms::Icon;
use crate::primitives::molecules::{FooterHintBar, render_filter_input_line, render_modal};
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
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            " Table Picker ",
            FooterHintBar::with_prefix(
                format!("{filtered_count} tables"),
                picker_hints(
                    Picker::Table,
                    HintSurface::Border,
                    &KeyContext::from_state(state),
                ),
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
