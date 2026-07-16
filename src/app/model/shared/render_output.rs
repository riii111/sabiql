use crate::model::shared::viewport::{ColumnWidthsCache, ViewportPlan};

/// Feedback measured during a draw. Owned by the model so state write-back
/// stays port-agnostic; the renderer port re-exports this type as its output.
#[derive(Default)]
pub struct RenderOutput {
    pub browse: BrowseRenderMetrics,
    pub input: InputRenderMetrics,
    pub pickers: PickersRenderMetrics,
    pub details: DetailRenderMetrics,
    pub overlays: OverlayRenderMetrics,
}

#[derive(Default)]
pub struct BrowseRenderMetrics {
    pub explorer: ExplorerRenderMetrics,
    pub inspector: InspectorRenderMetrics,
    pub result: ResultRenderMetrics,
}

#[derive(Default)]
pub struct ExplorerRenderMetrics {
    pub pane_height: u16,
    pub content_width: usize,
}

#[derive(Default)]
pub struct InspectorRenderMetrics {
    pub viewport_plan: ViewportPlan,
    pub pane_height: u16,
}

#[derive(Default)]
pub struct ResultRenderMetrics {
    pub viewport_plan: ViewportPlan,
    pub widths_cache: ColumnWidthsCache,
    pub pane_height: u16,
}

#[derive(Default)]
pub struct InputRenderMetrics {
    pub command_line_visible_width: Option<usize>,
}

#[derive(Default)]
pub struct PickersRenderMetrics {
    pub connection_list_pane_height: Option<u16>,
    pub table: Option<PickerRenderMetrics>,
    pub er: Option<PickerRenderMetrics>,
    pub query_history: Option<PickerRenderMetrics>,
}

pub struct PickerRenderMetrics {
    pub pane_height: u16,
    pub filter_visible_width: usize,
}

#[derive(Default)]
pub struct DetailRenderMetrics {
    pub jsonb: Option<JsonbDetailRenderMetrics>,
    pub row: Option<RowDetailRenderMetrics>,
}

pub struct JsonbDetailRenderMetrics {
    pub editor_visible_rows: usize,
}

pub struct RowDetailRenderMetrics {
    pub visible_rows: usize,
    pub visible_columns: usize,
}

#[derive(Default)]
pub struct OverlayRenderMetrics {
    pub confirm_preview: ConfirmPreviewRenderMetrics,
    pub explain_compare_viewport_height: Option<u16>,
}

#[derive(Default)]
pub struct ConfirmPreviewRenderMetrics {
    pub viewport_height: Option<u16>,
    pub content_height: Option<u16>,
    pub scroll: u16,
}
