use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Color;
use ratatui::widgets::Clear;

use crate::ui::primitives::molecules::overlay::{
    centered_rect, modal_block_with_hint, modal_block_with_hint_color, render_scrim,
};
use crate::ui::theme::ThemePalette;

pub fn render_modal(
    frame: &mut Frame,
    width: Constraint,
    height: Constraint,
    title: &str,
    hint: &str,
    theme: &ThemePalette,
) -> (Rect, Rect) {
    let area = centered_rect(frame.area(), width, height);

    render_scrim(frame, theme);
    frame.render_widget(Clear, area);

    let block = modal_block_with_hint(title.to_string(), hint.to_string(), theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    (area, inner)
}

pub fn render_modal_with_border_color(
    frame: &mut Frame,
    width: Constraint,
    height: Constraint,
    title: &str,
    hint: &str,
    border_color: Color,
    theme: &ThemePalette,
) -> (Rect, Rect) {
    let area = centered_rect(frame.area(), width, height);

    render_scrim(frame, theme);
    frame.render_widget(Clear, area);

    let block =
        modal_block_with_hint_color(title.to_string(), hint.to_string(), border_color, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    (area, inner)
}
