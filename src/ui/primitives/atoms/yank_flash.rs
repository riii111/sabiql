use ratatui::style::{Color, Style};
use ratatui::text::Line;

use crate::app::model::shared::flash_timer::FlashPhase;
use crate::primitives::utils::color::blend;
use crate::theme::ThemePalette;

pub fn apply_yank_flash(lines: &mut [Line], phase: FlashPhase, theme: &ThemePalette) {
    for line in lines {
        flash_line(line, phase, theme);
    }
}

pub fn apply_yank_flash_masked(
    lines: &mut [Line],
    phase: FlashPhase,
    mask: &[bool],
    theme: &ThemePalette,
) {
    debug_assert_eq!(
        lines.len(),
        mask.len(),
        "flash mask must align with rendered lines",
    );
    for (line, &should_flash) in lines.iter_mut().zip(mask.iter()) {
        if should_flash {
            flash_line(line, phase, theme);
        }
    }
}

fn flash_line(line: &mut Line, phase: FlashPhase, theme: &ThemePalette) {
    match phase {
        FlashPhase::Off => {}
        FlashPhase::Solid => {
            *line = std::mem::take(line).style(
                Style::default()
                    .fg(theme.component.feedback.yank_flash_fg)
                    .bg(theme.component.feedback.yank_flash_bg),
            );
        }
        FlashPhase::Afterglow(progress) => {
            let line_fg = line.style.fg.unwrap_or(theme.semantic.text.primary);
            for span in &mut line.spans {
                let settled = span.style.fg.unwrap_or(line_fg);
                span.style.fg = Some(afterglow_fg(settled, progress, theme));
            }
        }
    }
}

pub fn afterglow_fg(settled: Color, progress: f32, theme: &ThemePalette) -> Color {
    blend(theme.component.feedback.yank_flash_bg, settled, progress)
}

#[cfg(test)]
mod tests {
    use ratatui::text::Span;

    use super::*;
    use crate::theme::DEFAULT_THEME;

    #[test]
    fn afterglow_starts_in_flash_color_and_settles_on_span_color() {
        let settled = Color::Rgb(10, 20, 30);
        let line = || Line::from(Span::styled("x", Style::default().fg(settled)));

        let mut start = [line()];
        apply_yank_flash(&mut start, FlashPhase::Afterglow(0.0), &DEFAULT_THEME);
        let mut end = [line()];
        apply_yank_flash(&mut end, FlashPhase::Afterglow(1.0), &DEFAULT_THEME);

        assert_eq!(
            start[0].spans[0].style.fg,
            Some(DEFAULT_THEME.component.feedback.yank_flash_bg)
        );
        assert_eq!(end[0].spans[0].style.fg, Some(settled));
    }

    #[test]
    fn afterglow_leaves_background_untouched() {
        let mut lines = [Line::raw("x")];

        apply_yank_flash(&mut lines, FlashPhase::Afterglow(0.5), &DEFAULT_THEME);

        assert_eq!(lines[0].style.bg, None);
        assert_eq!(lines[0].spans[0].style.bg, None);
    }

    #[test]
    fn masked_afterglow_skips_unmasked_lines() {
        let mut lines = [Line::raw("a"), Line::raw("b")];

        apply_yank_flash_masked(
            &mut lines,
            FlashPhase::Afterglow(0.0),
            &[true, false],
            &DEFAULT_THEME,
        );

        assert!(lines[0].spans[0].style.fg.is_some());
        assert_eq!(lines[1].spans[0].style.fg, None);
    }
}
