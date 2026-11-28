use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Color;
use ratatui::widgets::Clear;

use crate::primitives::molecules::FooterHintBar;
use crate::primitives::molecules::overlay::{centered_rect, modal_block, render_scrim};
use crate::theme::ThemePalette;

pub fn render_modal(
    frame: &mut Frame,
    width: Constraint,
    height: Constraint,
    title: &str,
    hint: FooterHintBar,
    theme: &ThemePalette,
) -> (Rect, Rect) {
    render_modal_with_border_color(
        frame,
        width,
        height,
        title,
        hint,
        theme.component.modal.border,
        theme,
    )
}

pub fn render_modal_with_border_color(
    frame: &mut Frame,
    width: Constraint,
    height: Constraint,
    title: &str,
    hint: FooterHintBar,
    border_color: Color,
    theme: &ThemePalette,
) -> (Rect, Rect) {
    let area = centered_rect(frame.area(), width, height);

    render_scrim(frame, theme);
    frame.render_widget(Clear, area);

    let block = modal_block(title.to_string(), hint.line(theme), border_color, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    (area, inner)
}
