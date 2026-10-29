use crate::app::model::er_state::ErLayout;
use crate::app::ports::outbound::ErDiagramLayout;
use crate::domain::ErTableInfo;
use crate::features::overlays::er_diagram::layout_tables;

pub struct RataflowErLayout;

impl ErDiagramLayout for RataflowErLayout {
    fn layout(&self, tables: &[ErTableInfo], seed_tables: &[String]) -> ErLayout {
        layout_tables(tables, seed_tables)
    }
}
