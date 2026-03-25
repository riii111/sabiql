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
}

pub trait Renderer {
    fn draw(&mut self, state: &mut AppState, services: &AppServices) -> Result<RenderOutput>;
}
