use std::sync::Arc;
use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::viewport::{ColumnWidthsCache, ViewportPlan};
use crate::services::AppServices;

#[derive(Debug, Clone, thiserror::Error)]
pub enum RenderError {
    #[error("I/O error: {0}")]
    Io(#[source] Arc<std::io::Error>),
}

impl From<std::io::Error> for RenderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(Arc::new(error))
    }
}

pub type RenderResult<T> = Result<T, RenderError>;

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
    pub confirm_preview_viewport_height: Option<u16>,
    pub confirm_preview_content_height: Option<u16>,
    pub confirm_preview_scroll: u16,
    pub explain_compare_viewport_height: Option<u16>,
}

pub trait Renderer {
    fn draw(
        &mut self,
        state: &AppState,
        services: &AppServices,
        now: Instant,
    ) -> RenderResult<RenderOutput>;
}
