use crate::model::shared::viewport::{ColumnWidthsCache, ViewportPlan};

/// Pane geometry measured during a draw. Owned by the model so state
/// write-back (`AppState::apply_render_output`) stays port-agnostic; the
/// renderer port re-exports this type as its output.
#[derive(Default)]
pub struct RenderOutput {
    pub inspector_viewport_plan: ViewportPlan,
    pub result_viewport_plan: ViewportPlan,
    pub result_widths_cache: ColumnWidthsCache,
    pub explorer_pane_height: u16,
    pub explorer_content_width: usize,
    pub inspector_pane_height: u16,
    pub result_pane_height: u16,
    pub command_line_visible_width: Option<usize>,
    pub connection_list_pane_height: Option<u16>,
    pub table_picker_pane_height: Option<u16>,
    pub table_picker_filter_visible_width: Option<usize>,
    pub er_picker_pane_height: Option<u16>,
    pub er_picker_filter_visible_width: Option<usize>,
    pub query_history_picker_pane_height: Option<u16>,
    pub query_history_picker_filter_visible_width: Option<usize>,
    pub jsonb_detail_editor_visible_rows: Option<usize>,
    pub row_detail_content_visible_rows: Option<usize>,
    pub confirm_preview_viewport_height: Option<u16>,
    pub confirm_preview_content_height: Option<u16>,
    pub confirm_preview_scroll: u16,
    pub explain_compare_viewport_height: Option<u16>,
}
