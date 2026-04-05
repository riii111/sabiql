use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::model::app_state::AppState;
use crate::ui::primitives::atoms::text_cursor_spans;
use crate::ui::primitives::molecules::render_modal;
use crate::ui::theme::ThemePalette;

pub struct TablePicker;

pub struct TablePickerRenderMetrics {
    pub pane_height: u16,
    pub filter_visible_width: usize,
}

impl TablePicker {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> TablePickerRenderMetrics {
        let filtered_count = state.filtered_tables().len();
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            " Table Picker ",
            &format!(" {filtered_count} tables │ ↑↓ Navigate │ Enter Select "),
            theme,
        );

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        let raw_width = filter_area.width.saturating_sub(4) as usize; // "  > " prefix

        let input = &state.ui.table_picker.filter_input;
        let visible_width = if input.cursor() == input.char_count() {
            raw_width.saturating_sub(1)
        } else {
            raw_width
        };
        let cursor_spans = text_cursor_spans(
            input.content(),
            input.cursor(),
            input.viewport_offset(),
            visible_width,
            theme,
        );
        let mut spans = vec![Span::styled("  > ", Style::default().fg(theme.modal_title))];
        spans.extend(cursor_spans);
        let filter_line = Line::from(spans);

        frame.render_widget(Paragraph::new(filter_line), filter_area);

        let filtered = state.filtered_tables();
        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| {
                let content = format!("  {}", t.qualified_name());
                ListItem::new(content).style(Style::default().fg(theme.text_secondary))
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
        TablePickerRenderMetrics {
            pane_height: list_area.height,
            filter_visible_width: raw_width,
        }
    }
}
