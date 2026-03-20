use std::time::Instant;

use ratatui::style::Style;
use ratatui::text::Line;

use crate::ui::theme::Theme;

pub fn apply_yank_flash(lines: &mut [Line], flash_until: Option<Instant>) {
    let active = flash_until.is_some_and(|until| Instant::now() < until);
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
