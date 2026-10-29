use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::er_state::ErViewerCommand;
use crate::services::AppServices;

pub use crate::model::shared::render_output::{CellDetailViewport, RenderOutput};

#[derive(Debug, thiserror::Error)]
#[error("I/O error: {0}")]
pub struct RenderError(#[from] std::io::Error);

pub type RenderResult<T> = Result<T, RenderError>;

pub trait Renderer {
    fn draw(
        &mut self,
        state: &AppState,
        services: &AppServices,
        now: Instant,
    ) -> RenderResult<RenderOutput>;

    fn apply_er_viewer_command(&mut self, command: ErViewerCommand);
}
