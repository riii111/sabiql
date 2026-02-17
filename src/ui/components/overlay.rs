use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::widgets::{Block, Borders};

use crate::ui::theme::Theme;

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

/// Dims the background to make the modal "float" visually.
/// Uses DIM + dark foreground to suppress background borders
/// that would otherwise appear adjacent to modal borders.
pub fn render_scrim(frame: &mut Frame) {
    let buf = frame.buffer_mut();
    let area = buf.area;
    buf.set_style(
        area,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    );
}

pub fn modal_block_with_hint(title: String, hint: String) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(Theme::MODAL_TITLE)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(hint)
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(Theme::MODAL_BORDER))
        .style(Style::default())
}
