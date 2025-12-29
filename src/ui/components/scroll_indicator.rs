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
        .begin_symbol(Some("<"))
        .end_symbol(Some(">"))
        .thumb_style(Style::default().fg(Color::Yellow))
        .track_style(Style::default().fg(Color::DarkGray))
        .begin_style(if can_scroll_left { arrow_active } else { arrow_inactive })
        .end_style(if can_scroll_right { arrow_active } else { arrow_inactive });

    let mut scrollbar_state = ScrollbarState::default()
        .content_length(scrollable_range.saturating_add(1))
        .viewport_content_length(params.viewport_size)
        .position(params.position.min(scrollable_range));

    frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
}

/// Render a vertical scroll indicator at the bottom-right of an area.
/// NOTE: `area` should be the INNER area (without border).
pub fn render_vertical_scroll_indicator(
    frame: &mut Frame,
    area: Rect,
    current_start: usize,
    visible_count: usize,
    total: usize,
) {
    if total <= visible_count {
        return;
    }

    let current_end = (current_start + visible_count).min(total);
    let indicator = format!("[{}-{}/{}]", current_start + 1, current_end, total);

    let indicator_area = Rect {
        x: area.x + area.width.saturating_sub(indicator.len() as u16),
        y: area.y + area.height.saturating_sub(1),
        width: indicator.len() as u16,
        height: 1,
    };

    frame.render_widget(
        Paragraph::new(indicator).style(Style::default().fg(Color::DarkGray)),
        indicator_area,
    );
}

