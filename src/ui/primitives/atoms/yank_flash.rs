use ratatui::style::Style;
use ratatui::text::Line;

use crate::ui::theme::Theme;

pub fn apply_yank_flash(lines: &mut [Line], active: bool) {
    if !active {
        return;
    }
    let flash_style = Style::default()
        .fg(Theme::YANK_FLASH_FG)
        .bg(Theme::YANK_FLASH_BG);
    for line in lines {
        *line = std::mem::take(line).style(flash_style);
    }
}
