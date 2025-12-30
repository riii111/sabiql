#![allow(dead_code)]

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::widgets::{Block, Borders, Clear};

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

/// Clears the given area by rendering a Clear widget.
/// This should be called before rendering overlay content.
pub fn clear_area(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
}

/// Dims the background to make the modal "float" visually.
pub fn render_scrim(frame: &mut Frame) {
    let scrim = Block::default().style(Style::default().bg(Theme::SCRIM_BG));
    frame.render_widget(scrim, frame.area());
}

pub fn modal_block(title: String) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(Theme::MODAL_TITLE)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(Theme::MODAL_BORDER))
        .style(Style::default().bg(Theme::MODAL_BG))
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
        .style(Style::default().bg(Theme::MODAL_BG))
}

pub fn render_modal_frame(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    render_scrim(frame);
    frame.render_widget(Clear, area);
    let block = modal_block(format!(" {} ", title));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}
