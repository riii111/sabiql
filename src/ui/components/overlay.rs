#![allow(dead_code)]

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::widgets::Clear;

/// Creates a centered rectangle within the given area.
///
/// # Arguments
/// * `area` - The parent area to center within
/// * `width` - Width constraint for the centered rect
/// * `height` - Height constraint for the centered rect
///
/// # Returns
/// A `Rect` centered horizontally and vertically within `area`
pub fn centered_rect(area: Rect, width: Constraint, height: Constraint) -> Rect {
    let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([height]).flex(Flex::Center).areas(area);
    area
}

/// Clears the given area by rendering a Clear widget.
/// This should be called before rendering overlay content.
pub fn clear_area(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
}
