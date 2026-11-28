use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};

use crate::theme::ThemePalette;

pub fn centered_rect(area: Rect, width: Constraint, height: Constraint) -> Rect {
    let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([height]).flex(Flex::Center).areas(area);
    area
}

// Uses DIM + dark foreground to suppress background borders
// that would otherwise appear adjacent to modal borders.
pub fn render_scrim(frame: &mut Frame, theme: &ThemePalette) {
    let buf = frame.buffer_mut();
    let area = buf.area;
    buf.set_style(
        area,
        Style::default()
            .fg(theme.semantic.text.muted)
            .add_modifier(Modifier::DIM),
    );
}

pub fn render_modal_shadow(frame: &mut Frame, area: Rect, theme: &ThemePalette) {
    let buf = frame.buffer_mut();
    let bounds = buf.area;
    let shadow = Style::default().bg(theme.component.modal.shadow);
    let right_edge = Rect::new(area.right(), area.y + 1, 1, area.height);
    let bottom_edge = Rect::new(area.x + 1, area.bottom(), area.width, 1);
    for edge in [right_edge, bottom_edge] {
        buf.set_style(edge.intersection(bounds), shadow);
    }
}

pub fn modal_block(
    title: String,
    hint: Line<'static>,
    border_color: Color,
    theme: &ThemePalette,
) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(theme.modal_title_style())
        .title_bottom(hint)
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(border_color))
        .style(Style::default())
}
