use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
        let selected_count = state.ui.er_selected_tables.len();
        let total_count = state.tables().len();

        let (mode_label, targets_label, preview_color) = if selected_count == 0 {
            ("Invalid".to_string(), "—".to_string(), Color::Red)
        } else if selected_count == total_count {
            (
                "Full ER".to_string(),
                format!("all {} tables", total_count),
                Color::DarkGray,
            )
        } else if selected_count == 1 {
            let name = state.ui.er_selected_tables.iter().next().unwrap().clone();
            ("Partial ER".to_string(), name, Color::Green)
        } else {
            (
                "Partial ER".to_string(),
                format!("{} tables", selected_count),
                Color::Cyan,
            )
        };

        let output_label = if selected_count == 0 {
            "—".to_string()
        } else if selected_count == total_count {
            "er_full.dot".to_string()
        } else if selected_count == 1 {
            let name = state.ui.er_selected_tables.iter().next().unwrap();
            let safe: String = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("er_partial_{}.dot", safe)
        } else {
            let mut sorted: Vec<&String> = state.ui.er_selected_tables.iter().collect();
            sorted.sort();
            let mut hasher = DefaultHasher::new();
            sorted.hash(&mut hasher);
            let hash = format!("{:016x}", hasher.finish());
            format!("er_partial_multi_{}_{}.dot", selected_count, &hash[..8])
        };

        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            " ER Diagram ",
            &format!(
                " {}/{} selected │ Space Select │ ^A All │ Enter Generate │ Esc Cancel ",
                selected_count, total_count
            ),
        );

        let [filter_area, preview_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
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

        // 3-line execution preview
        let preview_lines = vec![
            Line::from(vec![
                Span::styled("  Mode:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(mode_label, Style::default().fg(preview_color)),
            ]),
            Line::from(vec![
                Span::styled("  Targets: ", Style::default().fg(Color::DarkGray)),
                Span::styled(targets_label, Style::default().fg(preview_color)),
            ]),
            Line::from(vec![
                Span::styled("  Output:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(output_label, Style::default().fg(preview_color)),
            ]),
        ];
        frame.render_widget(Paragraph::new(preview_lines), preview_area);

        // Table list with checkboxes
        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| {
                let qn = t.qualified_name();
                let is_selected = state.ui.er_selected_tables.contains(&qn);
                let mark = if is_selected { "✔ " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Gray)
                };
                ListItem::new(format!("  {}{}", mark, qn)).style(style)
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
                .er_picker_list_state
                .select(Some(state.ui.er_picker_selected));
        } else {
            state.ui.er_picker_list_state.select(None);
        }

        frame.render_stateful_widget(list, list_area, &mut state.ui.er_picker_list_state);
    }
}
