use std::collections::BTreeSet;

use ratatui::widgets::ListState;

use super::explorer_mode::ExplorerMode;
use super::focused_pane::FocusedPane;
use super::input_mode::InputMode;
use super::inspector_tab::InspectorTab;
use super::keybindings::HELP_TOTAL_LINES;
use super::viewport::ViewportPlan;

/// header (1) + scroll indicators (2), used by rendering (border already excluded)
pub const RESULT_INNER_OVERHEAD: u16 = 3;

/// border (2) + inner overhead, used by scroll limit calculation
pub const RESULT_PANE_OVERHEAD: u16 = 2 + RESULT_INNER_OVERHEAD;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultNavMode {
    Scroll,
    RowActive,
    CellActive,
}

/// Invariant: `cell` is `Some` only when `row` is `Some`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultSelection {
    row: Option<usize>,
    cell: Option<usize>,
}

impl ResultSelection {
    pub fn mode(&self) -> ResultNavMode {
        match (self.row, self.cell) {
            (Some(_), Some(_)) => ResultNavMode::CellActive,
            (Some(_), None) => ResultNavMode::RowActive,
            _ => ResultNavMode::Scroll,
        }
    }

    pub fn row(&self) -> Option<usize> {
        self.row
    }

    pub fn cell(&self) -> Option<usize> {
        self.cell
    }

    pub fn enter_row(&mut self, row: usize) {
        self.row = Some(row);
        self.cell = None;
    }

    /// Move row cursor while preserving the current cell selection.
    pub fn move_row(&mut self, row: usize) {
        self.row = Some(row);
    }

    pub fn enter_cell(&mut self, col: usize) {
        if self.row.is_some() {
            self.cell = Some(col);
        }
    }

    pub fn exit_to_row(&mut self) {
        self.cell = None;
    }

    pub fn reset(&mut self) {
        self.row = None;
        self.cell = None;
    }

    pub fn clamp(&mut self, max_rows: usize, max_cols: usize) {
        if max_rows == 0 {
            self.reset();
            return;
        }
        if let Some(r) = self.row
            && r >= max_rows
        {
            self.reset();
            return;
        }
        if max_cols == 0 {
            self.cell = None;
            return;
        }
        if let Some(c) = self.cell
            && c >= max_cols
        {
            self.cell = Some(max_cols - 1);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub focused_pane: FocusedPane,
    pub focus_mode: bool,
    pub focus_mode_prev_pane: Option<FocusedPane>,
    pub input_mode: InputMode,
    pub command_line_return_mode: InputMode,

    pub explorer_selected: usize,
    pub explorer_horizontal_offset: usize,
    pub explorer_list_state: ListState,
    pub explorer_mode: ExplorerMode,

    pub connection_list_selected: usize,
    pub connection_list_state: ListState,

    pub picker_selected: usize,
    pub picker_list_state: ListState,
    pub filter_input: String,

    pub er_filter_input: String,
    pub er_picker_selected: usize,
    pub er_picker_list_state: ListState,
    pub er_selected_tables: BTreeSet<String>,
    pub pending_er_picker: bool,

    pub inspector_tab: InspectorTab,
    pub inspector_scroll_offset: usize,
    pub inspector_horizontal_offset: usize,
    pub inspector_viewport_plan: ViewportPlan,
    pub inspector_pane_height: u16,

    pub explorer_pane_height: u16,

    pub result_scroll_offset: usize,
    pub result_horizontal_offset: usize,
    pub result_viewport_plan: ViewportPlan,
    pub result_pane_height: u16,
    pub result_selection: ResultSelection,

    pub help_scroll_offset: usize,

    pub terminal_height: u16,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            terminal_height: 24,
            ..Default::default()
        }
    }

    pub fn result_visible_rows(&self) -> usize {
        self.result_pane_height.saturating_sub(RESULT_PANE_OVERHEAD) as usize
    }

    pub fn inspector_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(5) as usize
    }

    /// Visible items in Explorer list (height minus 2 borders minus 1 scrollbar row)
    pub fn explorer_visible_items(&self) -> usize {
        self.explorer_pane_height.saturating_sub(3) as usize
    }

    pub fn inspector_ddl_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(3) as usize
    }

    /// Estimate max scroll for help overlay based on terminal height.
    /// Modal is 80% height with 2-line border, so viewport ≈ terminal_height * 0.8 - 2
    pub fn help_max_scroll(&self) -> usize {
        let viewport = (self.terminal_height as usize * 80 / 100).saturating_sub(2);
        HELP_TOTAL_LINES.saturating_sub(viewport)
    }

    pub fn toggle_focus(&mut self) -> bool {
        if self.focus_mode {
            if let Some(prev) = self.focus_mode_prev_pane.take() {
                self.focused_pane = prev;
            }
            self.focus_mode = false;
        } else {
            self.focus_mode_prev_pane = Some(self.focused_pane);
            self.focused_pane = FocusedPane::Result;
            self.focus_mode = true;
        }
        true
    }

    /// Update explorer selection, keeping explorer_selected and explorer_list_state in sync.
    /// For "no selection", `explorer_list_state.selected()` is the source of truth.
    pub fn set_explorer_selection(&mut self, index: Option<usize>) {
        match index {
            Some(i) => {
                self.explorer_selected = i;
                self.explorer_list_state.select(Some(i));
            }
            None => {
                self.explorer_selected = 0;
                self.explorer_list_state.select(None);
            }
        }
    }

    /// Update connection list selection, keeping connection_list_selected and connection_list_state in sync.
    pub fn set_connection_list_selection(&mut self, index: Option<usize>) {
        match index {
            Some(i) => {
                self.connection_list_selected = i;
                self.connection_list_state.select(Some(i));
            }
            None => {
                self.connection_list_selected = 0;
                self.connection_list_state.select(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn default_creates_empty_state() {
        let state = UiState::default();

        assert_eq!(state.focused_pane, FocusedPane::default());
        assert!(!state.focus_mode);
        assert!(state.focus_mode_prev_pane.is_none());
        assert_eq!(state.explorer_selected, 0);
        assert!(state.filter_input.is_empty());
    }

    #[test]
    fn new_sets_terminal_height() {
        let state = UiState::new();

        assert_eq!(state.terminal_height, 24);
    }

    #[test]
    fn default_result_pane_height_returns_zero_visible_rows() {
        let state = UiState::default();

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[rstest]
    #[case(10, 5)]
    #[case(15, 10)]
    #[case(20, 15)]
    #[case(30, 25)]
    fn result_pane_height_calculates_correct_visible_rows(
        #[case] pane_height: u16,
        #[case] expected: usize,
    ) {
        let state = UiState {
            result_pane_height: pane_height,
            ..Default::default()
        };

        let visible = state.result_visible_rows();

        assert_eq!(visible, expected);
    }

    #[test]
    fn small_result_pane_height_does_not_underflow() {
        let state = UiState {
            result_pane_height: 2,
            ..Default::default()
        };

        let visible = state.result_visible_rows();

        assert_eq!(visible, 0);
    }

    #[test]
    fn toggle_focus_enters_focus_mode() {
        let mut state = UiState {
            focused_pane: FocusedPane::Explorer,
            ..Default::default()
        };

        let result = state.toggle_focus();

        assert!(result);
        assert!(state.focus_mode);
        assert_eq!(state.focused_pane, FocusedPane::Result);
        assert_eq!(state.focus_mode_prev_pane, Some(FocusedPane::Explorer));
    }

    #[test]
    fn toggle_focus_exits_focus_mode_and_restores_pane() {
        let mut state = UiState {
            focused_pane: FocusedPane::Inspector,
            ..Default::default()
        };
        state.toggle_focus();

        let result = state.toggle_focus();

        assert!(result);
        assert!(!state.focus_mode);
        assert_eq!(state.focused_pane, FocusedPane::Inspector);
    }

    #[rstest]
    #[case(10, 7)]
    #[case(15, 12)]
    #[case(20, 17)]
    fn ddl_visible_rows_equals_height_minus_three(
        #[case] pane_height: u16,
        #[case] expected: usize,
    ) {
        let state = UiState {
            inspector_pane_height: pane_height,
            ..Default::default()
        };

        let visible = state.inspector_ddl_visible_rows();

        assert_eq!(visible, expected);
    }

    #[test]
    fn ddl_visible_rows_is_greater_than_standard() {
        let state = UiState {
            inspector_pane_height: 20,
            ..Default::default()
        };

        let standard = state.inspector_visible_rows();
        let ddl = state.inspector_ddl_visible_rows();

        assert_eq!(ddl - standard, 2);
    }

    #[test]
    fn set_explorer_selection_with_some_syncs_both_fields() {
        let mut state = UiState::default();

        state.set_explorer_selection(Some(5));

        assert_eq!(state.explorer_selected, 5);
        assert_eq!(state.explorer_list_state.selected(), Some(5));
    }

    #[test]
    fn set_explorer_selection_with_none_resets_to_zero_and_none() {
        let mut state = UiState::default();
        state.set_explorer_selection(Some(10));

        state.set_explorer_selection(None);

        assert_eq!(state.explorer_selected, 0);
        assert_eq!(state.explorer_list_state.selected(), None);
    }

    #[test]
    fn default_explorer_mode_is_tables() {
        let state = UiState::default();

        assert_eq!(state.explorer_mode, ExplorerMode::Tables);
    }

    #[test]
    fn set_connection_list_selection_with_some_syncs_both_fields() {
        let mut state = UiState::default();

        state.set_connection_list_selection(Some(3));

        assert_eq!(state.connection_list_selected, 3);
        assert_eq!(state.connection_list_state.selected(), Some(3));
    }

    #[test]
    fn set_connection_list_selection_with_none_resets_to_zero_and_none() {
        let mut state = UiState::default();
        state.set_connection_list_selection(Some(5));

        state.set_connection_list_selection(None);

        assert_eq!(state.connection_list_selected, 0);
        assert_eq!(state.connection_list_state.selected(), None);
    }

    #[test]
    fn result_overhead_constants_are_consistent() {
        assert_eq!(RESULT_PANE_OVERHEAD, RESULT_INNER_OVERHEAD + 2);
    }

    #[rstest]
    #[case(30, 20)]
    #[case(100, 25)]
    #[case(50, 15)]
    #[case(10, 30)]
    fn scroll_can_reach_all_rows(#[case] total_rows: usize, #[case] pane_height: u16) {
        let state = UiState {
            result_pane_height: pane_height,
            ..Default::default()
        };
        let visible = state.result_visible_rows();
        let max_scroll = total_rows.saturating_sub(visible);

        assert!(
            max_scroll + visible >= total_rows,
            "max_scroll={}, visible={}, total={}",
            max_scroll,
            visible,
            total_rows
        );
    }

    mod result_selection {
        use super::*;

        #[test]
        fn default_is_scroll_mode() {
            let sel = ResultSelection::default();

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
            assert!(sel.row().is_none());
            assert!(sel.cell().is_none());
        }

        #[test]
        fn enter_row_transitions_to_row_active() {
            let mut sel = ResultSelection::default();

            sel.enter_row(5);

            assert_eq!(sel.mode(), ResultNavMode::RowActive);
            assert_eq!(sel.row(), Some(5));
            assert!(sel.cell().is_none());
        }

        #[test]
        fn enter_cell_transitions_to_cell_active() {
            let mut sel = ResultSelection::default();
            sel.enter_row(3);

            sel.enter_cell(7);

            assert_eq!(sel.mode(), ResultNavMode::CellActive);
            assert_eq!(sel.row(), Some(3));
            assert_eq!(sel.cell(), Some(7));
        }

        #[test]
        fn enter_cell_without_row_is_noop() {
            let mut sel = ResultSelection::default();

            sel.enter_cell(5);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn exit_to_row_clears_cell_only() {
            let mut sel = ResultSelection::default();
            sel.enter_row(2);
            sel.enter_cell(4);

            sel.exit_to_row();

            assert_eq!(sel.mode(), ResultNavMode::RowActive);
            assert_eq!(sel.row(), Some(2));
        }

        #[test]
        fn reset_clears_both() {
            let mut sel = ResultSelection::default();
            sel.enter_row(1);
            sel.enter_cell(2);

            sel.reset();

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn enter_row_clears_previous_cell() {
            let mut sel = ResultSelection::default();
            sel.enter_row(0);
            sel.enter_cell(3);

            sel.enter_row(5);

            assert_eq!(sel.mode(), ResultNavMode::RowActive);
            assert_eq!(sel.row(), Some(5));
        }

        #[test]
        fn move_row_preserves_cell() {
            let mut sel = ResultSelection::default();
            sel.enter_row(0);
            sel.enter_cell(3);

            sel.move_row(5);

            assert_eq!(sel.mode(), ResultNavMode::CellActive);
            assert_eq!(sel.row(), Some(5));
            assert_eq!(sel.cell(), Some(3));
        }

        #[test]
        fn move_row_in_row_active_stays_row_active() {
            let mut sel = ResultSelection::default();
            sel.enter_row(2);

            sel.move_row(7);

            assert_eq!(sel.mode(), ResultNavMode::RowActive);
            assert_eq!(sel.row(), Some(7));
        }

        #[test]
        fn clamp_resets_when_zero_rows() {
            let mut sel = ResultSelection::default();
            sel.enter_row(5);

            sel.clamp(0, 10);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn clamp_resets_when_row_out_of_bounds() {
            let mut sel = ResultSelection::default();
            sel.enter_row(10);
            sel.enter_cell(2);

            sel.clamp(5, 10);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn clamp_caps_cell_to_max_cols() {
            let mut sel = ResultSelection::default();
            sel.enter_row(0);
            sel.enter_cell(9);

            sel.clamp(10, 5);

            assert_eq!(sel.cell(), Some(4));
        }

        #[test]
        fn clamp_clears_cell_when_zero_cols() {
            let mut sel = ResultSelection::default();
            sel.enter_row(0);
            sel.enter_cell(3);

            sel.clamp(10, 0);

            assert_eq!(sel.mode(), ResultNavMode::RowActive);
        }

        #[test]
        fn clamp_preserves_valid_selection() {
            let mut sel = ResultSelection::default();
            sel.enter_row(3);
            sel.enter_cell(2);

            sel.clamp(10, 10);

            assert_eq!(sel.row(), Some(3));
            assert_eq!(sel.cell(), Some(2));
        }
    }
}
