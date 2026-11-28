use ratatui::widgets::{Block, Borders};

use crate::theme::ThemePalette;

pub(crate) fn panel_block(title: &str, focused: bool, theme: &ThemePalette) -> Block<'static> {
    Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(theme.panel_border_style(focused, false))
}

pub(crate) fn panel_block_highlight(
    title: &str,
    focused: bool,
    highlight: bool,
    theme: &ThemePalette,
) -> Block<'static> {
    Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(theme.panel_border_style(focused, highlight))
}
