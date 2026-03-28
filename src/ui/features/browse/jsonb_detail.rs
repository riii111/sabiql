use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::policy::json::visible_line_indices;
use crate::ui::primitives::atoms::json_tree::json_tree_line_spans;
use crate::ui::primitives::molecules::render_modal;

pub struct JsonbDetail;

impl JsonbDetail {
    pub fn render(frame: &mut Frame, state: &AppState) {
        if !state.jsonb_detail.is_active() {
            return;
        }

        let title = format!(
            " JSONB Detail \u{2500}\u{2500} {} (jsonb) ",
            state.jsonb_detail.column_name()
        );
        let hint = " y:Copy  i:Edit  /:Search  j/k:Nav  h/l:Fold  Esc:Close ";

        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            &title,
            hint,
        );

        let tree = state.jsonb_detail.tree();
        let visible = visible_line_indices(tree);
        let selected = state.jsonb_detail.selected_line();
        let scroll = state.jsonb_detail.scroll_offset();
        let viewport_height = inner.height as usize;

        let lines: Vec<Line<'_>> = visible
            .iter()
            .skip(scroll)
            .take(viewport_height)
            .enumerate()
            .map(|(view_idx, &real_idx)| {
                let is_selected = (scroll + view_idx) == selected;
                json_tree_line_spans(&tree.lines()[real_idx], is_selected)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }
}
