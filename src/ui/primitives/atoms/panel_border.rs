use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};

use crate::ui::theme::Theme;

pub fn panel_block(title: &str, focused: bool) -> Block<'static> {
    let border_style = if focused {
        Style::default().fg(Theme::FOCUS_BORDER)
    } else {
        Style::default().fg(Theme::UNFOCUS_BORDER)
    };

    Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(border_style)
}

pub fn panel_block_highlight(title: &str, focused: bool, highlight: bool) -> Block<'static> {
    let border_style = if focused {
        Style::default().fg(Theme::FOCUS_BORDER)
    } else if highlight {
        Style::default().fg(Theme::HIGHLIGHT_BORDER)
    } else {
        Style::default().fg(Theme::UNFOCUS_BORDER)
    };

    Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(border_style)
}
