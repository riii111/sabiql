use crate::domain::ErTableInfo;
use crate::model::er_state::ErLayout;

pub trait ErDiagramLayout: Send + Sync {
    fn layout(&self, tables: &[ErTableInfo]) -> ErLayout;
}
