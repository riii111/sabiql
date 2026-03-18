use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::ui::primitives::atoms::scroll_indicator::{
    VerticalScrollParams, clamp_scroll_offset, render_vertical_scroll_indicator_bar,
};
use crate::ui::theme::Theme;

pub struct StripedTableConfig<'b> {
    pub headers: &'b [&'b str],
    pub widths: &'b [Constraint],
    pub total_items: usize,
    pub empty_message: &'b str,
}

pub fn render_striped_table<'a>(
    frame: &mut Frame,
    area: Rect,
    config: &StripedTableConfig<'_>,
    scroll_offset: usize,
    row_fn: impl Fn(usize) -> Vec<Cell<'a>>,
) {
    if config.total_items == 0 {
        frame.render_widget(Paragraph::new(config.empty_message), area);
        return;
    }

    let header = Row::new(config.headers.iter().map(|&h| Cell::from(h)))
        .style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED)
                .fg(Theme::TEXT_PRIMARY),
        )
        .height(1);

    // -2: header (1) + scroll indicator (1)
    let visible_rows = area.height.saturating_sub(2) as usize;
    let clamped_scroll_offset =
        clamp_scroll_offset(scroll_offset, visible_rows, config.total_items);

    let rows: Vec<Row> = (clamped_scroll_offset..config.total_items)
        .take(visible_rows)
        .enumerate()
        .map(|(visual_idx, item_idx)| {
            let style = if visual_idx % 2 == 1 {
                Style::default().bg(Theme::STRIPED_ROW_BG)
            } else {
                Style::default()
            };
            Row::new(row_fn(item_idx)).style(style)
        })
        .collect();

    let table_widget = Table::new(rows, config.widths).header(header);
    frame.render_widget(table_widget, area);

    render_vertical_scroll_indicator_bar(
        frame,
        area,
        VerticalScrollParams {
            position: clamped_scroll_offset,
            viewport_size: visible_rows,
            total_items: config.total_items,
        },
    );
}
