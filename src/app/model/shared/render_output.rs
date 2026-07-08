use crate::model::shared::viewport::{ColumnWidthsCache, ViewportPlan};

/// Pane geometry measured during a draw. Owned by the model so state
/// write-back (`AppState::apply_render_output`) stays port-agnostic; the
/// renderer port re-exports this type as its output.
#[derive(Default)]
pub struct RenderOutput {
    pub inspector_viewport_plan: ViewportPlan,
    pub result_viewport_plan: ViewportPlan,
    pub result_widths_cache: ColumnWidthsCache,
    /// Corrected/clamped `ResultInteraction::cell_vertical_offset`, computed
    /// against the active cell's actual wrapped-line count during this draw.
    /// Mirrors how `result_viewport_plan` self-corrects `horizontal_offset`.
    pub result_cell_vertical_offset: usize,
    /// Low Scroll Mode layout measured during this draw, or `None` when the
    /// mode is off. Carries the per-row line heights (and the active row's
    /// line bounds within the pane) so the scroll reducer can do line-based
    /// visibility math instead of the row-count math used by the normal
    /// table. Like `result_viewport_plan`, it reflects the previous frame.
    pub result_low_scroll_layout: Option<crate::model::shared::low_scroll::MeasuredLowScrollLayout>,
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
    pub row_detail_content_visible_columns: Option<usize>,
    pub confirm_preview_viewport_height: Option<u16>,
    pub confirm_preview_content_height: Option<u16>,
    pub confirm_preview_scroll: u16,
    pub explain_compare_viewport_height: Option<u16>,
}
