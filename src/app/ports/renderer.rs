use std::time::Instant;

use color_eyre::eyre::Result;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::viewport::{ColumnWidthsCache, ViewportPlan};
use crate::app::services::AppServices;

#[derive(Default)]
pub struct RenderOutput {
    pub inspector_viewport_plan: ViewportPlan,
    pub result_viewport_plan: ViewportPlan,
    pub result_widths_cache: ColumnWidthsCache,
    pub explorer_pane_height: u16,
    pub inspector_pane_height: u16,
    pub result_pane_height: u16,
    pub command_line_visible_width: Option<usize>,
    pub connection_list_pane_height: Option<u16>,
    pub confirm_preview_viewport_height: Option<u16>,
    pub confirm_preview_content_height: Option<u16>,
    pub confirm_preview_scroll: Option<u16>,
    pub explain_compare_viewport_height: Option<u16>,
}

pub trait Renderer {
    fn draw(
        &mut self,
        state: &mut AppState,
        services: &AppServices,
        now: Instant,
    ) -> Result<RenderOutput>;
}
