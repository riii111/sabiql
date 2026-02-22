use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::molecules::render_modal;

pub struct TablePicker;

impl TablePicker {
    pub fn render(frame: &mut Frame, state: &AppState) {
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
                    .fg(Theme::CURSOR_FG)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]);

        let filter_widget = Paragraph::new(filter_line);
        frame.render_widget(filter_widget, filter_area);

        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| {
                let content = format!("  {}", t.qualified_name());
                ListItem::new(content).style(Style::default().fg(Theme::TEXT_SECONDARY))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Theme::COMPLETION_SELECTED_BG)
                    .fg(Theme::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        let selected = if !filtered.is_empty() {
            Some(state.ui.picker_selected)
        } else {
            None
        };
        let mut list_state = ListState::default().with_selected(selected);
        frame.render_stateful_widget(list, list_area, &mut list_state);
    }
}
