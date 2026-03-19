use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::app::sql_modal_context::CompletionKind;
use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub(super) fn render_completion_popup(
    frame: &mut Frame,
    modal_area: Rect,
    editor_area: Rect,
    state: &AppState,
) {
    let (cursor_row, cursor_col) =
        super::cursor::cursor_to_position(&state.sql_modal.content, state.sql_modal.cursor);

    let max_items = 8;
    let visible_count = state.sql_modal.completion.candidates.len().min(max_items);
    let popup_height = (visible_count as u16) + 2;
    let popup_width = 45u16.min(modal_area.width);

    let popup_x = if modal_area.width < popup_width {
        modal_area.x
    } else {
        (editor_area.x + cursor_col as u16).min(modal_area.right().saturating_sub(popup_width))
    };
    let cursor_screen_y = editor_area.y + cursor_row as u16;

    let popup_y = if cursor_screen_y + 1 + popup_height > modal_area.bottom() {
        cursor_screen_y.saturating_sub(popup_height)
    } else {
        cursor_screen_y + 1
    };

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let selected = state.sql_modal.completion.selected_index;
    let total = state.sql_modal.completion.candidates.len();
    let scroll_offset = if total <= max_items {
        0
    } else {
        let half = max_items / 2;
        if selected < half {
            0
        } else if selected >= total - half {
            total - max_items
        } else {
            selected - half
        }
    };

    let max_text_width = state
        .sql_modal
        .completion
        .candidates
        .iter()
        .skip(scroll_offset)
        .take(max_items)
        .map(|c| c.text.len())
        .max()
        .unwrap_or(0);

    let items: Vec<ListItem> = state
        .sql_modal
        .completion
        .candidates
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_items)
        .map(|(i, candidate)| {
            let is_selected = i == selected;

            let kind_label = match candidate.kind {
                CompletionKind::Keyword => "keyword",
                CompletionKind::Table => "table",
                CompletionKind::Column => "column",
            };

            let padding = max_text_width.saturating_sub(candidate.text.len()) + 2;
            let text = format!(
                " {}{:padding$}{}",
                candidate.text,
                "",
                kind_label,
                padding = padding
            );

            let style = if is_selected {
                Style::default()
                    .bg(Theme::COMPLETION_SELECTED_BG)
                    .fg(Theme::TEXT_PRIMARY)
            } else {
                Style::default().fg(Theme::TEXT_SECONDARY)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::MODAL_BORDER))
            .style(Style::default()),
    );

    frame.render_widget(list, popup_area);
}
