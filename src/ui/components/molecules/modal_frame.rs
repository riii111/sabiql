use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::widgets::Clear;

use crate::ui::components::overlay::{centered_rect, modal_block_with_hint, render_scrim};

/// Renders a modal frame with scrim, clear, and border.
/// Returns the inner area for content rendering.
///
/// # Arguments
/// * `frame` - The frame to render to
/// * `width` - Width constraint (e.g., `Constraint::Percentage(60)`)
/// * `height` - Height constraint (e.g., `Constraint::Percentage(80)`)
/// * `title` - Modal title (displayed at top border)
/// * `hint` - Hint text (displayed at bottom border)
///
/// # Returns
/// The inner `Rect` where content should be rendered
pub fn render_modal(
    frame: &mut Frame,
    width: Constraint,
    height: Constraint,
    title: &str,
    hint: &str,
) -> Rect {
    let area = centered_rect(frame.area(), width, height);

    render_scrim(frame);
    frame.render_widget(Clear, area);

    let block = modal_block_with_hint(title.to_string(), hint.to_string());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    inner
}
