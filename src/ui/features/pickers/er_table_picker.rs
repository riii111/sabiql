use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::model::app_state::AppState;
use crate::domain::er::er_output_filename;
use crate::ui::primitives::atoms::text_cursor_spans;
use crate::ui::theme::Theme;

use crate::ui::primitives::molecules::render_modal;

pub struct ErTablePicker;

impl ErTablePicker {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let selected_count = state.ui.er_selected_tables.len();
        let total_count = state.tables().len();
        let filtered_count = state.er_filtered_tables().len();

        let (mode_label, targets_label, preview_color) = if selected_count == 0 {
            ("Invalid".to_string(), "—".to_string(), Theme::STATUS_ERROR)
        } else if selected_count == total_count {
            (
                "Full ER".to_string(),
                format!("all {total_count} tables"),
                Theme::TEXT_MUTED,
            )
        } else if selected_count == 1 {
            let name = state.ui.er_selected_tables.iter().next().unwrap().clone();
            ("Partial ER".to_string(), name, Theme::ACTIVE_INDICATOR)
        } else {
            (
                "Partial ER".to_string(),
                format!("{selected_count} tables"),
                Theme::SECTION_HEADER,
            )
        };

        let output_label = if selected_count == 0 {
            "—".to_string()
        } else {
            let selected_vec: Vec<String> = state.ui.er_selected_tables.iter().cloned().collect();
            er_output_filename(&selected_vec, total_count)
        };

        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(60),
            Constraint::Percentage(70),
            " ER Diagram ",
            &format!(
                " {selected_count}/{total_count} selected │ Space Select │ ^A All │ Enter Generate │ Esc Cancel "
            ),
        );

        let [filter_area, preview_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .areas(inner);

        state.ui.er_picker.pane_height = list_area.height;
        let raw_width = filter_area.width.saturating_sub(4) as usize;
        state.ui.er_picker.filter_visible_width = raw_width;

        // Filter input
        let input = &state.ui.er_picker.filter_input;
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
        );
        let mut spans = vec![Span::styled(
            "  > ",
            Style::default().fg(Theme::MODAL_TITLE),
        )];
        spans.extend(cursor_spans);
        frame.render_widget(Paragraph::new(Line::from(spans)), filter_area);

        // 3-line execution preview
        let preview_lines = vec![
            Line::from(vec![
                Span::styled("  Mode:    ", Style::default().fg(Theme::TEXT_MUTED)),
                Span::styled(mode_label, Style::default().fg(preview_color)),
            ]),
            Line::from(vec![
                Span::styled("  Targets: ", Style::default().fg(Theme::TEXT_MUTED)),
                Span::styled(targets_label, Style::default().fg(preview_color)),
            ]),
            Line::from(vec![
                Span::styled("  Output:  ", Style::default().fg(Theme::TEXT_MUTED)),
                Span::styled(output_label, Style::default().fg(preview_color)),
            ]),
        ];
        frame.render_widget(Paragraph::new(preview_lines), preview_area);

        // Table list with checkboxes
        let filtered = state.er_filtered_tables();
        let items: Vec<ListItem> = filtered
            .iter()
            .map(|t| {
                let qn = t.qualified_name();
                let is_selected = state.ui.er_selected_tables.contains(&qn);
                let mark = if is_selected { "✔ " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(Theme::ACTIVE_INDICATOR)
                } else {
                    Style::default().fg(Theme::TEXT_SECONDARY)
                };
                ListItem::new(format!("  {mark}{qn}")).style(style)
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

        let selected = if filtered_count > 0 {
            Some(state.ui.er_picker.selected())
        } else {
            None
        };
        let mut list_state = ListState::default()
            .with_selected(selected)
            .with_offset(state.ui.er_picker.scroll_offset());
        frame.render_stateful_widget(list, list_area, &mut list_state);
    }
}
