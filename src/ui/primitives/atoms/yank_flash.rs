use ratatui::style::Style;
use ratatui::text::Line;

use crate::ui::theme::ThemePalette;

pub fn apply_yank_flash(lines: &mut [Line], active: bool, theme: &ThemePalette) {
    if !active {
        return;
    }
    let flash_style = Style::default()
        .fg(theme.yank_flash_fg)
        .bg(theme.yank_flash_bg);
    for line in lines {
        *line = std::mem::take(line).style(flash_style);
    }
}
