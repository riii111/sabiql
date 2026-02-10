use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::molecules::render_modal;

pub struct ErTablePicker;

impl ErTablePicker {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let filtered = state.er_filtered_tables();
        let is_full_er = state.ui.er_filter_input.is_empty();

        let preview = if is_full_er {
            let total = state.tables().len();
            format!("▶ Full ER (all {} tables)", total)
        } else if let Some(table) = filtered.get(state.ui.er_picker_selected) {
            format!("▶ Partial ER from {}", table.qualified_name())
        } else {
            "▶ No matching tables".to_string()
        };

        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            " ER Diagram ",
            &format!(
                " {} tables │ ↑↓ Navigate │ Enter Generate │ Esc Cancel ",
                filtered.len()
            ),
        );

        let [filter_area, preview_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);

        // Filter input
        let filter_line = Line::from(vec![
            Span::styled("  > ", Style::default().fg(Theme::MODAL_TITLE)),
            Span::raw(&state.ui.er_filter_input),
            Span::styled(
                "█",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]);
        frame.render_widget(Paragraph::new(filter_line), filter_area);

        // Preview line
        let preview_color = if is_full_er {
            Color::DarkGray
        } else if filtered.is_empty() {
            Color::Red
        } else {
            Color::Green
        };
        let preview_line = Line::from(Span::styled(
            format!("  {}", preview),
            Style::default().fg(preview_color),
        ));
        frame.render_widget(Paragraph::new(preview_line), preview_area);

        // Table list
        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| {
                let content = format!("  {}", t.qualified_name());
                ListItem::new(content).style(Style::default().fg(Color::Gray))
            })
            .collect();

        let list = if is_full_er {
            // No highlight when empty filter (selection doesn't affect result)
            List::new(items)
        } else {
            List::new(items)
                .highlight_style(
                    Style::default()
                        .bg(Theme::COMPLETION_SELECTED_BG)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▸ ")
        };

        if !is_full_er && !filtered.is_empty() {
            state
                .ui
                .er_picker_list_state
                .select(Some(state.ui.er_picker_selected));
        } else {
            state.ui.er_picker_list_state.select(None);
        }

        frame.render_stateful_widget(list, list_area, &mut state.ui.er_picker_list_state);
    }
}
