use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::molecules::render_modal;

pub struct TablePicker;

impl TablePicker {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let filtered = state.filtered_tables();
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            " Table Picker ",
            &format!(" {} tables │ ↑↓ Navigate │ Enter Select ", filtered.len()),
        );

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        let filter_line = Line::from(vec![
            Span::styled("  > ", Style::default().fg(Theme::MODAL_TITLE)),
            Span::raw(&state.ui.filter_input),
            Span::styled(
                "█",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]);

        let filter_widget = Paragraph::new(filter_line);
        frame.render_widget(filter_widget, filter_area);

        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| {
                let content = format!("  {}", t.qualified_name());
                ListItem::new(content).style(Style::default().fg(Color::Gray))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Theme::COMPLETION_SELECTED_BG)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        if !filtered.is_empty() {
            state
                .ui
                .picker_list_state
                .select(Some(state.ui.picker_selected));
        } else {
            state.ui.picker_list_state.select(None);
        }

        frame.render_stateful_widget(list, list_area, &mut state.ui.picker_list_state);
    }
}
