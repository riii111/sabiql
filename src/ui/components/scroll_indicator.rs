use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

pub struct HorizontalScrollParams {
    pub position: usize,
    pub viewport_size: usize,
    pub total_items: usize,
}

/// Render a horizontal scroll indicator at the bottom of an area.
/// NOTE: `area` should be the INNER area (without border).
pub fn render_horizontal_scroll_indicator(
    frame: &mut Frame,
    area: Rect,
    params: HorizontalScrollParams,
) {
    if params.total_items <= params.viewport_size {
        return;
    }

    let can_scroll_left = params.position > 0;
    let can_scroll_right = params.position + params.viewport_size < params.total_items;

    if !can_scroll_left && !can_scroll_right {
        return;
    }

    let left_margin: u16 = 1;
    let available_width = area.width.saturating_sub(left_margin) as usize;
    if available_width < 15 {
        return;
    }

    let scrollable_range = params.total_items.saturating_sub(params.viewport_size);
    let percentage = if scrollable_range > 0 {
        (params.position * 100) / scrollable_range
    } else {
        0
    };
    let position_text = format!("col {:>3}%", percentage.min(100));

    // Layout: [col XXX%][space][scrollbar with < and >]
    let fixed_parts_len = position_text.len() + 1;
    let scrollbar_width = available_width.saturating_sub(fixed_parts_len).max(5);

    use ratatui::text::{Line, Span};
    let text_style = Style::default().fg(Color::Yellow);

    let position_span = Span::styled(format!("{} ", position_text), text_style);

    let text_line = Line::from(vec![position_span]);

    let text_area = Rect {
        x: area.x + left_margin,
        y: area.y + area.height.saturating_sub(1),
        width: (text_line.width() as u16).min(area.width),
        height: 1,
    };
    frame.render_widget(Paragraph::new(text_line), text_area);

    let scrollbar_area = Rect {
        x: text_area.x + text_area.width,
        y: text_area.y,
        width: scrollbar_width as u16,
        height: 1,
    };

    let arrow_active = Style::default().fg(Color::Yellow);
    let arrow_inactive = Style::default().fg(Color::DarkGray);

    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .thumb_symbol("═")
        .track_symbol(Some("─"))
        .begin_symbol(Some("◀︎"))
        .end_symbol(Some("▶︎"))
        .thumb_style(Style::default().fg(Color::Yellow))
        .track_style(Style::default().fg(Color::DarkGray))
        .begin_style(if can_scroll_left {
            arrow_active
        } else {
            arrow_inactive
        })
        .end_style(if can_scroll_right {
            arrow_active
        } else {
            arrow_inactive
        });

    // Workaround for Ratatui scrollbar bug (issue #1681):
    // content_length should be the scrollable range, not total items
    let scrollable_range = params.total_items.saturating_sub(params.viewport_size);
    let mut scrollbar_state = ScrollbarState::default()
        .content_length(scrollable_range)
        .viewport_content_length(0) // Use default (track size)
        .position(params.position);

    frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
}

pub struct VerticalScrollParams {
    pub position: usize,
    pub viewport_size: usize,
    pub total_items: usize,
}

/// Render a vertical scrollbar on the right side of an area.
/// NOTE: `area` should be the INNER area (without border).
pub fn render_vertical_scroll_indicator_bar(
    frame: &mut Frame,
    area: Rect,
    params: VerticalScrollParams,
) {
    if params.total_items <= params.viewport_size {
        return;
    }

    let can_scroll_up = params.position > 0;
    let can_scroll_down = params.position + params.viewport_size < params.total_items;

    if !can_scroll_up && !can_scroll_down {
        return;
    }

    // Need at least 3 rows for scrollbar (arrow + thumb + arrow)
    if area.height < 3 {
        return;
    }

    let scrollable_range = params.total_items.saturating_sub(params.viewport_size);
    let percentage = if scrollable_range > 0 {
        (params.position * 100) / scrollable_range
    } else {
        0
    };

    // Render position text at bottom-right
    let position_text = format!("row {:>3}%", percentage.min(100));
    let text_style = Style::default().fg(Color::Yellow);
    let text_width = position_text.len() as u16;

    let text_area = Rect {
        x: area.x + area.width.saturating_sub(text_width),
        y: area.y + area.height.saturating_sub(1),
        width: text_width,
        height: 1,
    };
    frame.render_widget(Paragraph::new(position_text).style(text_style), text_area);

    // Render scrollbar on the right edge (above the position text)
    let scrollbar_height = area.height.saturating_sub(1); // Reserve 1 row for position text
    if scrollbar_height < 3 {
        return;
    }

    let scrollbar_area = Rect {
        x: area.x + area.width.saturating_sub(1),
        y: area.y,
        width: 1,
        height: scrollbar_height,
    };

    let arrow_active = Style::default().fg(Color::Yellow);
    let arrow_inactive = Style::default().fg(Color::DarkGray);

    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .thumb_symbol("┃")
        .track_symbol(Some("│"))
        .begin_symbol(Some("▲"))
        .end_symbol(Some("▼"))
        .thumb_style(Style::default().fg(Color::Yellow))
        .track_style(Style::default().fg(Color::DarkGray))
        .begin_style(if can_scroll_up {
            arrow_active
        } else {
            arrow_inactive
        })
        .end_style(if can_scroll_down {
            arrow_active
        } else {
            arrow_inactive
        });

    // Workaround for Ratatui scrollbar bug (issue #1681):
    // content_length should be the scrollable range, not total items
    let scrollable_range = params.total_items.saturating_sub(params.viewport_size);
    let mut scrollbar_state = ScrollbarState::default()
        .content_length(scrollable_range)
        .viewport_content_length(0) // Use default (track size)
        .position(params.position);

    frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
}
