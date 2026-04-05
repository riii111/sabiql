use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};

use crate::ui::theme::ThemePalette;

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
            .fg(theme.text_muted)
            .add_modifier(Modifier::DIM),
    );
}

pub fn modal_block_with_hint(title: String, hint: String, theme: &ThemePalette) -> Block<'static> {
    modal_block_with_hint_color(title, hint, theme.modal_border, theme)
}

pub fn modal_block_with_hint_color(
    title: String,
    hint: String,
    border_color: Color,
    theme: &ThemePalette,
) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(theme.modal_title_style())
        .title_bottom(Line::styled(hint, theme.modal_hint_style()))
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(border_color))
        .style(Style::default())
}
