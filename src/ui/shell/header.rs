use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::model::app_state::AppState;
use crate::domain::MetadataState;
use crate::primitives::utils::text_utils::truncate_to_width_with;
use crate::theme::ThemePalette;

const LEFT_SEPARATOR: &str = " ▸ ";
const RIGHT_SEPARATOR: &str = " | ";

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

        let left_items = vec![
            HeaderItem::new(&state.runtime.project_name, item_style, 2),
            HeaderItem::new(db_name, item_style, 1),
            HeaderItem::new(table, Style::default().fg(theme.semantic.text.primary), 0),
        ];

        let mut right_items = vec![HeaderItem::new(
            status_text,
            Style::default().fg(status_color),
            2,
        )];
        if let Some(effective_user) = state.session.effective_user() {
            right_items.push(HeaderItem::new(
                &format!("user: {effective_user}"),
                item_style,
                1,
            ));
        }
        right_items.push(HeaderItem::new(
            state
                .session
                .active_connection_name
                .as_deref()
                .unwrap_or("-"),
            item_style,
            0,
        ));
        if state.session.read_only {
            right_items.push(HeaderItem::new(
                "READ-ONLY",
                Style::default()
                    .fg(Color::Black)
                    .bg(theme.semantic.status.warning)
                    .add_modifier(Modifier::BOLD),
                3,
            ));
        }

        let layout = layout_header(left_items, right_items, area.width as usize);
        if layout.left_width > 0 {
            frame.render_widget(
                Paragraph::new(header_line(layout.left_items, LEFT_SEPARATOR, sep_style)),
                Rect::new(area.x, area.y, layout.left_width as u16, area.height),
            );
        }
        if layout.right_width > 0 {
            let right_x = area.x.saturating_add(layout.right_start as u16);
            frame.render_widget(
                Paragraph::new(header_line(layout.right_items, RIGHT_SEPARATOR, sep_style)),
                Rect::new(right_x, area.y, layout.right_width as u16, area.height),
            );
        }
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

struct HeaderLayout {
    left_items: Vec<HeaderItem>,
    right_items: Vec<HeaderItem>,
    left_width: usize,
    right_width: usize,
    right_start: usize,
}

fn layout_header(
    left_items: Vec<HeaderItem>,
    right_items: Vec<HeaderItem>,
    max_width: usize,
) -> HeaderLayout {
    let right_items = fit_header_items(right_items, max_width, RIGHT_SEPARATOR);
    let right_width = header_items_width(&right_items, RIGHT_SEPARATOR);
    let gap_width = usize::from(right_width > 0 && right_width < max_width);
    let left_max_width = max_width.saturating_sub(right_width + gap_width);
    let left_items = fit_header_items(left_items, left_max_width, LEFT_SEPARATOR);
    let left_width = header_items_width(&left_items, LEFT_SEPARATOR);

    HeaderLayout {
        left_items,
        right_items,
        left_width,
        right_width,
        right_start: max_width.saturating_sub(right_width),
    }
}

fn header_line(items: Vec<HeaderItem>, separator: &str, separator_style: Style) -> Line<'static> {
    let mut line = Line::from(Vec::with_capacity(items.len() * 2));
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            line.push_span(Span::styled(separator.to_string(), separator_style));
        }
        line.push_span(Span::styled(item.content, item.style));
    }
    line
}

fn fit_header_items(
    mut items: Vec<HeaderItem>,
    max_width: usize,
    separator: &str,
) -> Vec<HeaderItem> {
    let mut keep = vec![true; items.len()];
    let mut item_count = items.len();
    let mut removal_order: Vec<usize> = (0..items.len()).collect();
    removal_order.sort_by_key(|&index| items[index].truncation_rank);
    for index in removal_order {
        if min_header_width(item_count, UnicodeWidthStr::width(separator)) <= max_width {
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

    let separators_width =
        UnicodeWidthStr::width(separator).saturating_mul(items.len().saturating_sub(1));
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

fn header_items_width(items: &[HeaderItem], separator: &str) -> usize {
    UnicodeWidthStr::width(
        items
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>()
            .join(separator)
            .as_str(),
    )
}

fn min_header_width(item_count: usize, separator_width: usize) -> usize {
    item_count.saturating_add(separator_width.saturating_mul(item_count.saturating_sub(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(content: &str, truncation_rank: usize) -> HeaderItem {
        HeaderItem::new(content, Style::default(), truncation_rank)
    }

    fn left_items() -> Vec<HeaderItem> {
        vec![item("project", 2), item("database", 1), item("table", 0)]
    }

    fn right_items() -> Vec<HeaderItem> {
        vec![item("connected", 2), item("READ-ONLY", 3)]
    }

    fn contents(items: &[HeaderItem]) -> Vec<&str> {
        items.iter().map(|item| item.content.as_str()).collect()
    }

    fn assert_layout_fits(layout: &HeaderLayout, max_width: usize) {
        let gap_width = usize::from(layout.right_width > 0 && layout.right_width < max_width);

        assert_eq!(
            layout.left_width,
            header_items_width(&layout.left_items, LEFT_SEPARATOR)
        );
        assert_eq!(
            layout.right_width,
            header_items_width(&layout.right_items, RIGHT_SEPARATOR)
        );
        assert!(layout.left_width + gap_width <= layout.right_start);
        assert!(layout.right_start + layout.right_width <= max_width);
        assert!(layout.left_width + gap_width + layout.right_width <= max_width);
    }

    #[test]
    fn keeps_right_group_at_the_right_edge_when_table_length_changes() {
        let right_items = vec![
            item("connected", 2),
            item("user: postgres", 1),
            item("connection", 0),
        ];
        let short_table = layout_header(
            vec![item("project", 2), item("database", 1), item("users", 0)],
            right_items,
            80,
        );
        let long_table = layout_header(
            vec![
                item("project", 2),
                item("database", 1),
                item("public.a_very_long_table_name", 0),
            ],
            vec![
                item("connected", 2),
                item("user: postgres", 1),
                item("connection", 0),
            ],
            80,
        );

        assert_eq!(short_table.right_start, long_table.right_start);
        assert_eq!(short_table.right_width, long_table.right_width);
        assert_ne!(
            long_table.left_items[2].content,
            short_table.left_items[2].content
        );
    }

    #[test]
    fn preserves_read_only_before_status_at_narrow_width() {
        let items = fit_header_items(
            vec![item("connected", 2), item("READ-ONLY", 3)],
            4,
            RIGHT_SEPARATOR,
        );

        assert_eq!(items.len(), 1);
        assert!(items[0].content.starts_with('R'));
        assert!(UnicodeWidthStr::width(items[0].content.as_str()) <= 4);
    }

    #[test]
    fn measures_unicode_and_never_exceeds_width() {
        let items = fit_header_items(vec![item("接続名", 0)], 4, RIGHT_SEPARATOR);

        assert!(UnicodeWidthStr::width(items[0].content.as_str()) <= 4);
    }

    #[test]
    fn handles_extreme_widths_without_overlap() {
        let left_required_width = header_items_width(&left_items(), LEFT_SEPARATOR);
        let right_required_width = header_items_width(&right_items(), RIGHT_SEPARATOR);
        let boundary = left_required_width + right_required_width + 1;
        let boundary_widths = [boundary - 1, boundary, boundary + 1];
        let boundary_layouts =
            boundary_widths.map(|width| layout_header(left_items(), right_items(), width));

        for (&width, layout) in boundary_widths.iter().zip(&boundary_layouts) {
            assert_layout_fits(layout, width);

            let repeated = layout_header(left_items(), right_items(), width);
            assert_eq!(contents(&layout.left_items), contents(&repeated.left_items));
            assert_eq!(
                contents(&layout.right_items),
                contents(&repeated.right_items)
            );
        }

        assert_eq!(
            contents(&boundary_layouts[0].left_items),
            vec!["project", "database", "tab…"]
        );
        assert_eq!(
            contents(&boundary_layouts[0].right_items),
            vec!["connected", "READ-ONLY"]
        );
        for layout in &boundary_layouts[1..] {
            assert_eq!(
                contents(&layout.left_items),
                vec!["project", "database", "table"]
            );
            assert_eq!(
                contents(&layout.right_items),
                vec!["connected", "READ-ONLY"]
            );
        }

        for width in [0, 1, 2, 4, boundary + 12] {
            let layout = layout_header(left_items(), right_items(), width);
            assert_layout_fits(&layout, width);
        }
    }

    #[test]
    fn drops_items_when_separators_cannot_fit() {
        let items = fit_header_items(
            vec![
                item("project", 3),
                item("database", 2),
                item("table", 1),
                item("connected", usize::MAX),
                item("connection", 0),
            ],
            2,
            RIGHT_SEPARATOR,
        );
        let text = items
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>()
            .join(RIGHT_SEPARATOR);

        assert_eq!(items.len(), 1);
        assert!(UnicodeWidthStr::width(text.as_str()) <= 2);
    }
}
