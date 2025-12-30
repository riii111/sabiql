use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::overlay::{centered_rect, modal_block_with_hint, render_scrim};

pub struct TablePicker;

impl TablePicker {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(60),
            Constraint::Percentage(70),
        );

        render_scrim(frame);
        frame.render_widget(Clear, area);

        let filtered = state.filtered_tables();
        let block = modal_block_with_hint(
            " Table Picker ".to_string(),
            format!(" {} tables │ ↑↓ Navigate │ Enter Select ", filtered.len()),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        let filter_line = Line::from(vec![
            Span::styled("  > ", Style::default().fg(Theme::MODAL_TITLE)),
            Span::raw(&state.filter_input),
            Span::styled("█", Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK)),
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
            state.picker_list_state.select(Some(state.picker_selected));
        } else {
            state.picker_list_state.select(None);
        }

        frame.render_stateful_widget(list, list_area, &mut state.picker_list_state);
    }
}
