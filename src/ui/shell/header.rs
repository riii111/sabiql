use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::model::app_state::AppState;
use crate::domain::MetadataState;
use crate::primitives::utils::text_utils::truncate_to_width_with;
use crate::theme::ThemePalette;

const HEADER_SEPARATOR_WIDTH: usize = 3;

pub struct Header;

impl Header {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let db_name = state.session.database_name().unwrap_or("-");
        let table = state.session.selected_table_key().unwrap_or("-");

        let sep_style = Style::default().fg(theme.semantic.text.muted);
        let item_style = Style::default().fg(theme.semantic.text.secondary);

        let (status_text, status_color) = if state.session.dsn.is_none() {
            ("no dsn", theme.semantic.status.error)
        } else {
            match &state.session.metadata_state() {
                MetadataState::Loaded => ("connected", theme.semantic.status.success),
                MetadataState::Loading => ("loading...", theme.semantic.status.warning),
                MetadataState::Error(_) => ("error", theme.semantic.status.error),
                MetadataState::NotLoaded => ("not loaded", theme.semantic.text.muted),
            }
        };

        let mut items = vec![
            HeaderItem::new(&state.runtime.project_name, item_style, 3),
            HeaderItem::new(db_name, item_style, 2),
            HeaderItem::new(table, Style::default().fg(theme.semantic.text.primary), 1),
            HeaderItem::new(status_text, Style::default().fg(status_color), usize::MAX),
        ];
        if let Some(effective_user) = state.session.effective_user() {
            items.push(HeaderItem::new(
                &format!("user: {effective_user}"),
                item_style,
                4,
            ));
        }
        items.push(HeaderItem::new(
            state
                .session
                .active_connection_name
                .as_deref()
                .unwrap_or("-"),
            item_style,
            0,
        ));
        if state.session.read_only {
            items.push(HeaderItem::new(
                "READ-ONLY",
                Style::default().fg(theme.semantic.status.warning),
                usize::MAX - 1,
            ));
        }

        let items = fit_header_items(items, area.width as usize);
        let mut line = Line::from(Vec::with_capacity(items.len() * 2));
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                line.push_span(Span::styled(" | ", sep_style));
            }
            line.push_span(Span::styled(item.content, item.style));
        }

        frame.render_widget(Paragraph::new(line), area);
    }
}

struct HeaderItem {
    content: String,
    style: Style,
    truncation_rank: usize,
}

impl HeaderItem {
    fn new(content: &str, style: Style, truncation_rank: usize) -> Self {
        Self {
            content: content.to_string(),
            style,
            truncation_rank,
        }
    }
}

fn fit_header_items(mut items: Vec<HeaderItem>, max_width: usize) -> Vec<HeaderItem> {
    let mut keep = vec![true; items.len()];
    let mut item_count = items.len();
    let mut removal_order: Vec<usize> = (0..items.len()).collect();
    removal_order.sort_by_key(|&index| items[index].truncation_rank);
    for index in removal_order {
        if min_header_width(item_count) <= max_width {
            break;
        }
        keep[index] = false;
        item_count -= 1;
    }
    items = items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| keep[index].then_some(item))
        .collect();

    let separators_width = HEADER_SEPARATOR_WIDTH.saturating_mul(items.len().saturating_sub(1));
    let mut widths: Vec<usize> = items
        .iter()
        .map(|item| UnicodeWidthStr::width(item.content.as_str()))
        .collect();
    let total_width = separators_width + widths.iter().sum::<usize>();
    let mut overflow = total_width.saturating_sub(max_width);

    let mut truncation_order: Vec<usize> = (0..items.len()).collect();
    truncation_order.sort_by_key(|&index| items[index].truncation_rank);
    for index in truncation_order {
        if overflow == 0 {
            break;
        }
        let reducible = widths[index].saturating_sub(1);
        let reduction = reducible.min(overflow);
        widths[index] -= reduction;
        overflow -= reduction;
    }

    for (item, width) in items.iter_mut().zip(widths) {
        item.content = truncate_to_width_with(&item.content, width, "…");
    }
    items
}

fn min_header_width(item_count: usize) -> usize {
    item_count.saturating_add(HEADER_SEPARATOR_WIDTH.saturating_mul(item_count.saturating_sub(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_connection_name_before_effective_user() {
        let items = fit_header_items(
            vec![
                HeaderItem::new("project", Style::default(), 3),
                HeaderItem::new("database", Style::default(), 2),
                HeaderItem::new("public.long_table_name", Style::default(), 1),
                HeaderItem::new("connected", Style::default(), usize::MAX),
                HeaderItem::new("user: postgres", Style::default(), 4),
                HeaderItem::new("very-long-connection-name", Style::default(), 0),
            ],
            50,
        );
        let text = items
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>()
            .join(" | ");

        assert!(text.contains("user: postgres"));
        assert!(UnicodeWidthStr::width(text.as_str()) <= 50);
    }

    #[test]
    fn drops_items_when_separators_cannot_fit() {
        let items = fit_header_items(
            vec![
                HeaderItem::new("project", Style::default(), 3),
                HeaderItem::new("database", Style::default(), 2),
                HeaderItem::new("table", Style::default(), 1),
                HeaderItem::new("connected", Style::default(), usize::MAX),
                HeaderItem::new("connection", Style::default(), 0),
            ],
            2,
        );
        let text = items
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>()
            .join(" | ");

        assert_eq!(items.len(), 1);
        assert!(UnicodeWidthStr::width(text.as_str()) <= 2);
    }
}
