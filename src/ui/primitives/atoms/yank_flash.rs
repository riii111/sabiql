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

pub fn apply_yank_flash_masked(
    lines: &mut [Line],
    active: bool,
    mask: &[bool],
    theme: &ThemePalette,
) {
    debug_assert_eq!(
        lines.len(),
        mask.len(),
        "flash mask must align with rendered lines",
    );
    if !active {
        return;
    }
    let flash_style = Style::default()
        .fg(theme.yank_flash_fg)
        .bg(theme.yank_flash_bg);
    for (line, &should_flash) in lines.iter_mut().zip(mask.iter()) {
        if should_flash {
            *line = std::mem::take(line).style(flash_style);
        }
    }
}
