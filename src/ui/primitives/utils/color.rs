use ratatui::style::Color;

const MIDPOINT: f32 = 0.5;

pub(crate) fn blend(from: Color, to: Color, progress: f32) -> Color {
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => Color::Rgb(
            mix(r1, r2, progress),
            mix(g1, g2, progress),
            mix(b1, b2, progress),
        ),
        // Indexed and named colors have no portable RGB value, so they switch at the midpoint instead of blending.
        _ if progress < MIDPOINT => from,
        _ => to,
    }
}

fn mix(from: u8, to: u8, progress: f32) -> u8 {
    let from = f32::from(from);
    (f32::from(to) - from).mul_add(progress, from).round() as u8
}

#[cfg(test)]
mod tests {
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
