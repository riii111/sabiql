use color_eyre::eyre::Result;

use crate::app::state::AppState;
use crate::app::viewport::ViewportPlan;

#[derive(Default)]
pub struct RenderOutput {
    pub inspector_viewport_plan: ViewportPlan,
    pub result_viewport_plan: ViewportPlan,
    pub inspector_pane_height: u16,
    pub result_pane_height: u16,
}

pub trait Renderer {
    fn draw(&mut self, state: &mut AppState) -> Result<RenderOutput>;
}
