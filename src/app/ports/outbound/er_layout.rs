use crate::domain::ErTableInfo;
use crate::model::er_state::ErLayout;

pub trait ErDiagramLayout: Send + Sync {
    /// `seed_tables` are the tables picked for a partial diagram; the layout
    /// focuses them initially. Empty for a full diagram.
    fn layout(&self, tables: &[ErTableInfo], seed_tables: &[String]) -> ErLayout;
}
