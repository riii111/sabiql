use ratatui::buffer::{Buffer, Cell};
use ratatui::style::{Color, Modifier};

use crate::app::model::shared::motion::FocusTransition;
use crate::app::model::shared::render_output::{PaneArea, PaneAreas};
use crate::theme::ThemePalette;

// The scrim's DIM cannot be interpolated, so it switches on halfway through the fade.
const SCRIM_SWITCH_PROGRESS: f32 = 0.5;

pub(super) fn apply_focus_transition(
    buf: &mut Buffer,
    areas: &PaneAreas,
    transition: FocusTransition,
    theme: &ThemePalette,
) {
    let focus = theme.semantic.surface.focus_border;
    let unfocus = theme.semantic.surface.unfocus_border;
    let progress = transition.progress;

    if let Some(area) = areas.area(transition.to) {
        recolor_border(buf, area, focus, blend(unfocus, focus, progress));
    }
    if let Some(area) = areas.area(transition.from) {
        recolor_border(buf, area, unfocus, blend(focus, unfocus, progress));
    }
}

fn recolor_border(buf: &mut Buffer, area: PaneArea, settled: Color, color: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let right = area.x + area.width - 1;
    let bottom = area.y + area.height - 1;
    let top_and_bottom = (area.x..=right).flat_map(|x| [(x, area.y), (x, bottom)]);
    let sides = (area.y..=bottom).flat_map(|y| [(area.x, y), (right, y)]);

    for position in top_and_bottom.chain(sides) {
        // Only cells still drawn in the settled border color change, so titles and highlights keep their own colors.
        if let Some(cell) = buf.cell_mut(position)
            && cell.fg == settled
        {
            cell.fg = color;
        }
    }
}

/// `background` is the frame as drawn before any overlay.
pub(super) fn apply_modal_fade(
    buf: &mut Buffer,
    background: &Buffer,
    progress: f32,
    theme: &ThemePalette,
) {
    let area = buf.area.intersection(background.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let (Some(cell), Some(under)) = (buf.cell_mut((x, y)), background.cell((x, y))) else {
                continue;
            };
            if *cell == *under {
                continue;
            }
            if is_scrim_cell(cell, under, theme) {
                fade_scrim_cell(cell, under, progress, theme);
            } else {
                fade_overlay_cell(cell, progress, theme);
            }
        }
    }
}

fn is_scrim_cell(cell: &Cell, under: &Cell, theme: &ThemePalette) -> bool {
    // The scrim only recolors and dims what was drawn before.
    // Any other change, such as a blank cell cleared inside the modal, belongs to the modal itself.
    cell.symbol() == under.symbol()
        && cell.fg == theme.semantic.text.muted
        && cell.modifier == under.modifier | Modifier::DIM
        && cell.bg == under.bg
}

fn fade_scrim_cell(cell: &mut Cell, under: &Cell, progress: f32, theme: &ThemePalette) {
    let default_fg = theme.semantic.text.primary;
    let fg = blend(
        resolve(under.fg, default_fg),
        resolve(cell.fg, default_fg),
        progress,
    );
    if progress < SCRIM_SWITCH_PROGRESS {
        cell.modifier = under.modifier;
    }
    cell.fg = fg;
}

fn fade_overlay_cell(cell: &mut Cell, progress: f32, theme: &ThemePalette) {
    // Default-colored text has no known RGB value, so only themed colors fade.
    if matches!(cell.fg, Color::Rgb(..)) {
        cell.fg = blend(theme.semantic.text.muted, cell.fg, progress);
    }
}

fn resolve(color: Color, default: Color) -> Color {
    if color == Color::Reset {
        default
    } else {
        color
    }
}

fn blend(from: Color, to: Color, progress: f32) -> Color {
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => Color::Rgb(
            mix(r1, r2, progress),
            mix(g1, g2, progress),
            mix(b1, b2, progress),
        ),
        // Indexed and named colors have no portable RGB value, so they switch at the midpoint instead of blending.
        _ if progress < SCRIM_SWITCH_PROGRESS => from,
        _ => to,
    }
}

fn mix(from: u8, to: u8, progress: f32) -> u8 {
    let from = f32::from(from);
    (f32::from(to) - from).mul_add(progress, from).round() as u8
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::style::{Modifier, Style};

    use super::*;
    use crate::app::model::shared::focused_pane::FocusedPane;
    use crate::theme::DEFAULT_THEME;

    const THEME: &ThemePalette = &DEFAULT_THEME;

    fn buffer() -> Buffer {
        Buffer::empty(Rect::new(0, 0, 10, 6))
    }

    mod blend_colors {
        use super::*;

        #[test]
        fn rgb_endpoints_match_inputs() {
            let from = Color::Rgb(0, 100, 200);
            let to = Color::Rgb(200, 100, 0);

            assert_eq!(blend(from, to, 0.0), from);
            assert_eq!(blend(from, to, 1.0), to);
        }

        #[test]
        fn rgb_midpoint_averages_channels() {
            assert_eq!(
                blend(Color::Rgb(0, 100, 200), Color::Rgb(200, 100, 0), 0.5),
                Color::Rgb(100, 100, 100)
            );
        }

        #[test]
        fn named_colors_switch_at_midpoint() {
            assert_eq!(blend(Color::Red, Color::Blue, 0.4), Color::Red);
            assert_eq!(blend(Color::Red, Color::Blue, 0.6), Color::Blue);
        }
    }

    mod focus_transition {
        use super::*;

        fn areas() -> PaneAreas {
            PaneAreas {
                explorer: Some(PaneArea {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 6,
                }),
                inspector: None,
                result: Some(PaneArea {
                    x: 4,
                    y: 0,
                    width: 6,
                    height: 6,
                }),
            }
        }

        fn bordered_buffer() -> Buffer {
            let mut buf = buffer();
            let surface = THEME.semantic.surface;
            buf.set_style(
                Rect::new(0, 0, 4, 6),
                Style::default().fg(surface.unfocus_border),
            );
            buf.set_style(
                Rect::new(4, 0, 6, 6),
                Style::default().fg(surface.focus_border),
            );
            buf
        }

        #[test]
        fn borders_start_from_previous_focus_colors() {
            let mut buf = bordered_buffer();
            let transition = FocusTransition {
                from: FocusedPane::Explorer,
                to: FocusedPane::Result,
                progress: 0.0,
            };

            apply_focus_transition(&mut buf, &areas(), transition, THEME);

            let surface = THEME.semantic.surface;
            assert_eq!(buf[(4, 0)].fg, surface.unfocus_border);
            assert_eq!(buf[(0, 0)].fg, surface.focus_border);
        }

        #[test]
        fn pane_interior_keeps_its_colors() {
            let mut buf = bordered_buffer();
            let transition = FocusTransition {
                from: FocusedPane::Explorer,
                to: FocusedPane::Result,
                progress: 0.0,
            };

            apply_focus_transition(&mut buf, &areas(), transition, THEME);

            assert_eq!(buf[(6, 2)].fg, THEME.semantic.surface.focus_border);
        }

        #[test]
        fn cells_in_other_colors_are_left_alone() {
            let mut buf = bordered_buffer();
            buf[(5, 0)].fg = THEME.semantic.text.accent;
            let transition = FocusTransition {
                from: FocusedPane::Explorer,
                to: FocusedPane::Result,
                progress: 0.0,
            };

            apply_focus_transition(&mut buf, &areas(), transition, THEME);

            assert_eq!(buf[(5, 0)].fg, THEME.semantic.text.accent);
        }
    }

    mod modal_fade {
        use super::*;

        fn background() -> Buffer {
            let mut buf = buffer();
            buf.set_string(
                0,
                0,
                "abcdefghij",
                Style::default().fg(Color::Rgb(200, 200, 200)),
            );
            buf
        }

        fn scrimmed(background: &Buffer) -> Buffer {
            let mut buf = background.clone();
            buf.set_style(
                buf.area,
                Style::default()
                    .fg(THEME.semantic.text.muted)
                    .add_modifier(Modifier::DIM),
            );
            buf
        }

        #[test]
        fn early_scrim_keeps_background_without_dim() {
            let background = background();
            let mut buf = scrimmed(&background);

            apply_modal_fade(&mut buf, &background, 0.0, THEME);

            assert_eq!(buf[(0, 0)].fg, Color::Rgb(200, 200, 200));
            assert!(!buf[(0, 0)].modifier.contains(Modifier::DIM));
        }

        #[test]
        fn late_scrim_applies_dim() {
            let background = background();
            let mut buf = scrimmed(&background);

            apply_modal_fade(&mut buf, &background, 0.9, THEME);

            assert!(buf[(0, 0)].modifier.contains(Modifier::DIM));
        }

        #[test]
        fn overlay_text_starts_muted() {
            let background = background();
            let mut buf = scrimmed(&background);
            buf.set_string(
                0,
                1,
                "Help",
                Style::default().fg(THEME.semantic.text.accent),
            );

            apply_modal_fade(&mut buf, &background, 0.0, THEME);

            assert_eq!(buf[(0, 1)].fg, THEME.semantic.text.muted);
        }

        #[test]
        fn cleared_modal_cells_keep_their_own_style() {
            let mut background = background();
            background.set_style(
                Rect::new(0, 2, 10, 1),
                Style::default()
                    .bg(Color::Rgb(30, 30, 40))
                    .add_modifier(Modifier::UNDERLINED),
            );
            let mut buf = scrimmed(&background);
            buf[(3, 2)].reset();

            apply_modal_fade(&mut buf, &background, 0.0, THEME);

            assert_eq!(buf[(3, 2)], Cell::default());
        }

        #[test]
        fn untouched_cells_stay_identical() {
            let background = background();
            let mut buf = background.clone();

            apply_modal_fade(&mut buf, &background, 0.0, THEME);

            assert_eq!(buf, background);
        }
    }
}
