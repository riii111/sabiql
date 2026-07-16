use crate::model::shared::viewport::{ColumnWidthsCache, ViewportPlan};

/// Layout data produced during a draw. Owned by the model so state write-back
/// stays port-agnostic; the renderer port re-exports this type as its output.
#[derive(Default)]
pub struct RenderOutput {
    pub browse: BrowseLayout,
    pub input: InputLayout,
    pub pickers: PickerLayouts,
    pub details: DetailLayout,
    pub overlays: OverlayLayout,
}

#[derive(Default)]
pub struct BrowseLayout {
    pub explorer: ExplorerLayout,
    pub inspector: InspectorLayout,
    pub result: ResultLayout,
}

#[derive(Default)]
pub struct ExplorerLayout {
    pub pane_height: u16,
    pub content_width: usize,
}

#[derive(Default)]
pub struct InspectorLayout {
    pub viewport_plan: ViewportPlan,
    pub pane_height: u16,
}

#[derive(Default)]
pub struct ResultLayout {
    pub viewport_plan: ViewportPlan,
    pub widths_cache: ColumnWidthsCache,
    pub pane_height: u16,
}

#[derive(Default)]
pub struct InputLayout {
    pub command_line_visible_width: Option<usize>,
}

#[derive(Default)]
pub struct PickerLayouts {
    pub connection_list_pane_height: Option<u16>,
    pub table: Option<PickerLayout>,
    pub er: Option<PickerLayout>,
    pub query_history: Option<PickerLayout>,
}

pub struct PickerLayout {
    pub pane_height: u16,
    pub filter_visible_width: usize,
}

#[derive(Default)]
pub struct DetailLayout {
    pub jsonb: Option<JsonbDetailLayout>,
    pub row: Option<RowDetailLayout>,
}

pub struct JsonbDetailLayout {
    pub editor_visible_rows: usize,
}

pub struct RowDetailLayout {
    pub visible_rows: usize,
    pub visible_columns: usize,
}

#[derive(Default)]
pub struct OverlayLayout {
    pub confirm_preview: ConfirmPreviewLayout,
    pub explain_compare_viewport_height: Option<u16>,
}

#[derive(Default)]
pub struct ConfirmPreviewLayout {
    pub viewport_height: Option<u16>,
    pub content_height: Option<u16>,
    pub scroll: u16,
}
