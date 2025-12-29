use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

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

    let available_width = area.width as usize;
    if available_width < 15 {
        return;
    }

    // Build components
    let left_arrow = if can_scroll_left { "<" } else { " " };
    let right_arrow = if can_scroll_right { ">" } else { " " };
    let position_text = format!("col {}-{}/{}", current_start + 1, current_end, total);

    // Format: "< col X-Y/Z ───█─── >"
    let fixed_parts_len = 1 + 1 + position_text.len() + 1 + 1 + 1;
    let track_width = available_width.saturating_sub(fixed_parts_len).max(5);

    let scrollbar = build_scrollbar_track(current_start, current_end, total, track_width);

    let indicator = format!(
        "{} {} {} {}",
        left_arrow, position_text, scrollbar, right_arrow
    );

    // Render at bottom-left of inner area with left margin
    let indicator_area = Rect {
        x: area.x + 1,
        y: area.y + area.height.saturating_sub(1),
        width: (indicator.len() as u16).min(area.width.saturating_sub(1)),
        height: 1,
    };

    let style = Style::default().fg(Color::Yellow);
    frame.render_widget(Paragraph::new(indicator).style(style), indicator_area);
}

/// Build a scrollbar track string (e.g., "───█────")
fn build_scrollbar_track(start: usize, end: usize, total: usize, width: usize) -> String {
    if total <= 1 || width < 3 {
        return "─".repeat(width);
    }

    let thumb_start = (start * width) / total;
    let thumb_end = ((end * width) / total).max(thumb_start + 1).min(width);

    (0..width)
        .map(|i| {
            if i >= thumb_start && i < thumb_end {
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
        let result = build_scrollbar_track(0, 2, 10, 10);
        assert!(result.starts_with("██"));
        assert!(result.ends_with("─"));
    }

    #[test]
    fn scrollbar_track_shows_thumb_at_end() {
        let result = build_scrollbar_track(8, 10, 10, 10);
        assert!(result.starts_with("─"));
        assert!(result.ends_with("██"));
    }

    #[test]
    fn scrollbar_track_shows_thumb_in_middle() {
        let result = build_scrollbar_track(4, 6, 10, 10);
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(chars[0], '─');
        assert!(chars.contains(&'█'));
        assert_eq!(chars[9], '─');
    }

    #[test]
    fn scrollbar_track_minimum_width() {
        let result = build_scrollbar_track(0, 1, 10, 2);
        assert_eq!(result, "──");
    }
}
