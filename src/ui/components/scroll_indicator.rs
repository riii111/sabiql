use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

pub struct HorizontalScrollParams {
    pub position: usize,
    pub viewport_size: usize,
    pub total_items: usize,
    pub display_start: usize,
    pub display_end: usize,
}

/// Render a horizontal scroll indicator at the bottom of an area.
/// NOTE: `area` should be the INNER area (without border).
pub fn render_horizontal_scroll_indicator(
    frame: &mut Frame,
    area: Rect,
    current_start: usize,
    current_end: usize,
    total: usize,
) {
    if total <= 1 {
        return;
    }

    let can_scroll_left = current_start > 0;
    let can_scroll_right = current_end < total;

    if !can_scroll_left && !can_scroll_right {
        return;
    }

    // Reserve 1 char for left margin, generate string to fit remaining width
    let left_margin: u16 = 1;
    let available_width = area.width.saturating_sub(left_margin) as usize;
    if available_width < 15 {
        return;
    }

    let position_text = format!("col {}-{}/{}", current_start + 1, current_end, total);

    // Format: "< col X-Y/Z ───█─── >"
    let fixed_parts_len = 1 + 1 + position_text.len() + 1 + 1 + 1;
    let track_width = available_width.saturating_sub(fixed_parts_len).max(5);

    let scrollbar = build_scrollbar_track(current_start, total, track_width);

    // Always show arrows (grayed out when can't scroll in that direction)
    use ratatui::text::{Line, Span};
    let arrow_active = Style::default().fg(Color::Yellow);
    let arrow_inactive = Style::default().fg(Color::DarkGray);
    let text_style = Style::default().fg(Color::Yellow);

    let line = Line::from(vec![
        Span::styled("<", if can_scroll_left { arrow_active } else { arrow_inactive }),
        Span::styled(format!(" {} ", position_text), text_style),
        Span::styled(&scrollbar, text_style),
        Span::styled(" >", if can_scroll_right { arrow_active } else { arrow_inactive }),
    ]);

    let indicator_area = Rect {
        x: area.x + left_margin,
        y: area.y + area.height.saturating_sub(1),
        width: (line.width() as u16).min(available_width as u16),
        height: 1,
    };

    frame.render_widget(Paragraph::new(line), indicator_area);
}

fn build_scrollbar_track(position: usize, total: usize, width: usize) -> String {
    if total <= 1 || width < 3 {
        return "─".repeat(width);
    }

    let thumb_size = if width >= 10 { 2 } else { 1 };
    let thumb_pos = (position * (width - thumb_size)) / total.max(1);

    (0..width)
        .map(|i| {
            if i >= thumb_pos && i < thumb_pos + thumb_size {
                '█'
            } else {
                '─'
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_track_shows_thumb_at_start() {
        let result = build_scrollbar_track(0, 10, 10);
        assert!(result.starts_with("██"));
    }

    #[test]
    fn scrollbar_track_shows_thumb_at_end() {
        let result = build_scrollbar_track(10, 10, 10);
        assert!(result.ends_with("██"));
    }

    #[test]
    fn scrollbar_track_shows_thumb_in_middle() {
        let result = build_scrollbar_track(5, 10, 10);
        let chars: Vec<char> = result.chars().collect();
        assert!(chars.contains(&'█'));
        assert_eq!(chars[0], '─');
    }

    #[test]
    fn scrollbar_track_minimum_width() {
        let result = build_scrollbar_track(0, 10, 2);
        assert_eq!(result, "──");
    }
}
