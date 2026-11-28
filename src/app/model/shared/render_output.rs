use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::viewport::{ColumnWidthsCache, ViewportPlan};

#[derive(Clone, Copy)]
pub struct CellDetailViewport {
    pub visible_rows: usize,
    pub viewport_width: usize,
}

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
    pub pane_areas: PaneAreas,
}

/// Absolute terminal cells, matching the coordinates of mouse events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneArea {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl PaneArea {
    pub fn contains(self, column: u16, row: u16) -> bool {
        column >= self.x
            && row >= self.y
            && u32::from(column) < u32::from(self.x) + u32::from(self.width)
            && u32::from(row) < u32::from(self.y) + u32::from(self.height)
    }
}

/// A pane hidden by focus mode has no area.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneAreas {
    pub explorer: Option<PaneArea>,
    pub inspector: Option<PaneArea>,
    pub result: Option<PaneArea>,
}

impl PaneAreas {
    pub fn area(&self, pane: FocusedPane) -> Option<PaneArea> {
        match pane {
            FocusedPane::Explorer => self.explorer,
            FocusedPane::Inspector => self.inspector,
            FocusedPane::Result => self.result,
        }
    }

    pub fn pane_at(&self, column: u16, row: u16) -> Option<FocusedPane> {
        [
            FocusedPane::Explorer,
            FocusedPane::Inspector,
            FocusedPane::Result,
        ]
        .into_iter()
        .find(|&pane| {
            self.area(pane)
                .is_some_and(|area| area.contains(column, row))
        })
    }
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
    pub json: Option<JsonDetailLayout>,
    pub cell: Option<CellDetailViewport>,
    pub row: Option<RowDetailLayout>,
}

pub struct JsonDetailLayout {
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
    pub sqlite_diagnostics_content_line_count: Option<usize>,
    pub sqlite_diagnostics_viewport_height: Option<usize>,
}

#[derive(Default)]
pub struct ConfirmPreviewLayout {
    pub viewport_height: Option<u16>,
    pub content_height: Option<u16>,
    pub scroll: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_areas() -> PaneAreas {
        PaneAreas {
            explorer: Some(PaneArea {
                x: 0,
                y: 1,
                width: 20,
                height: 30,
            }),
            inspector: Some(PaneArea {
                x: 20,
                y: 1,
                width: 60,
                height: 15,
            }),
            result: Some(PaneArea {
                x: 20,
                y: 16,
                width: 60,
                height: 15,
            }),
        }
    }

    #[test]
    fn pane_at_finds_pane_under_cell() {
        let areas = split_areas();

        assert_eq!(areas.pane_at(5, 10), Some(FocusedPane::Explorer));
        assert_eq!(areas.pane_at(20, 1), Some(FocusedPane::Inspector));
        assert_eq!(areas.pane_at(79, 30), Some(FocusedPane::Result));
    }

    #[test]
    fn pane_at_ignores_cells_outside_panes() {
        let areas = split_areas();

        assert_eq!(areas.pane_at(5, 0), None);
        assert_eq!(areas.pane_at(80, 10), None);
        assert_eq!(areas.pane_at(5, 31), None);
    }

    #[test]
    fn hidden_pane_is_never_hit() {
        let areas = PaneAreas {
            explorer: None,
            ..split_areas()
        };

        assert_eq!(areas.pane_at(5, 10), None);
    }
}
