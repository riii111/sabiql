use std::collections::BTreeSet;
use std::time::Instant;

use super::focused_pane::FocusedPane;

use super::inspector_tab::InspectorTab;
use super::key_sequence::KeySequenceState;
use super::picker::PickerState;
use super::theme_id::ThemeId;
use super::viewport::{ColumnWidthsCache, ViewportPlan};
use crate::app::update::input::keybindings::help_content_line_count;
use unicode_width::UnicodeWidthStr;

pub use super::picker::clamp_scroll_offset;

// header (1) + scroll indicators (2), used by rendering (border already excluded)
pub const RESULT_INNER_OVERHEAD: u16 = 3;

// border (2) + inner overhead, used by scroll limit calculation
pub const RESULT_PANE_OVERHEAD: u16 = 2 + RESULT_INNER_OVERHEAD;
pub const EXPLORER_PANEL_BORDER_WIDTH: u16 = 2;
pub const EXPLORER_HIGHLIGHT_SYMBOL_WIDTH: u16 = 2;
pub const EXPLORER_SCROLLBAR_RESERVED_WIDTH: u16 = 1;
// Help modal height as a percent of the available terminal height.
pub const HELP_MODAL_HEIGHT_PERCENT: u16 = 80;
// Top and bottom modal border rows subtracted from the inner visible area.
pub const MODAL_VERTICAL_BORDER_OVERHEAD: usize = 2;
pub const DEFAULT_JSONB_DETAIL_EDITOR_VISIBLE_ROWS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultNavMode {
    Scroll,
    CellActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusMode {
    #[default]
    Normal,
    Focused {
        previous_pane: FocusedPane,
    },
}

impl FocusMode {
    pub fn focused(previous_pane: FocusedPane) -> Self {
        Self::Focused { previous_pane }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Focused { .. })
    }

    pub fn previous_pane(self) -> Option<FocusedPane> {
        match self {
            Self::Focused { previous_pane } => Some(previous_pane),
            Self::Normal => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct YankFlash {
    pub row: usize,
    pub col: Option<usize>,
    pub until: Instant,
}

// Invariant: `row` and `cell` are both `Some` for CellActive, or both `None` for Scroll.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultSelection {
    row: Option<usize>,
    cell: Option<usize>,
}

impl ResultSelection {
    fn is_consistent(&self) -> bool {
        self.row.is_some() == self.cell.is_some()
    }

    pub fn mode(&self) -> ResultNavMode {
        if self.row.is_some() && self.cell.is_some() {
            ResultNavMode::CellActive
        } else {
            ResultNavMode::Scroll
        }
    }

    pub fn row(&self) -> Option<usize> {
        if self.is_consistent() { self.row } else { None }
    }

    pub fn cell(&self) -> Option<usize> {
        if self.is_consistent() {
            self.cell
        } else {
            None
        }
    }

    pub fn enter_cell(&mut self, row: usize, col: usize) {
        self.row = Some(row);
        self.cell = Some(col);
        debug_assert!(self.is_consistent());
    }

    pub fn move_row(&mut self, row: usize) {
        debug_assert!(self.is_consistent());
        if self.cell.is_some() {
            self.row = Some(row);
        }
        debug_assert!(self.is_consistent());
    }

    pub fn move_cell(&mut self, col: usize) {
        debug_assert!(self.is_consistent());
        if self.row.is_some() {
            self.cell = Some(col);
        }
        debug_assert!(self.is_consistent());
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
            self.reset();
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
    theme_id: ThemeId,
    pub focused_pane: FocusedPane,
    pub focus_mode: FocusMode,
    pub explorer_selected: usize,
    pub explorer_scroll_offset: usize,
    pub explorer_horizontal_offset: usize,
    // Default::default() leaves this at 0 until the first render updates it, so
    // scroll_max_offset may temporarily return the full content width.
    pub explorer_content_width: usize,

    pub connection_list_selected: usize,
    pub connection_list_scroll_offset: usize,
    pub connection_list_pane_height: u16,

    pub table_picker: PickerState,

    pub er_picker: PickerState,
    pub er_selected_tables: BTreeSet<String>,
    pub pending_er_picker: bool,

    pub inspector_tab: InspectorTab,
    pub inspector_scroll_offset: usize,
    pub inspector_horizontal_offset: usize,
    pub inspector_viewport_plan: ViewportPlan,
    pub inspector_pane_height: u16,

    pub explorer_pane_height: u16,

    pub result_viewport_plan: ViewportPlan,
    pub result_widths_cache: ColumnWidthsCache,
    pub result_pane_height: u16,
    pub jsonb_detail_editor_visible_rows: usize,

    pub help_scroll_offset: usize,

    pub terminal_height: u16,

    pub key_sequence: KeySequenceState,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            terminal_height: 24,
            jsonb_detail_editor_visible_rows: DEFAULT_JSONB_DETAIL_EDITOR_VISIBLE_ROWS,
            ..Default::default()
        }
    }

    pub fn is_focus_mode(&self) -> bool {
        self.focus_mode.is_active()
    }

    pub fn theme_id(&self) -> ThemeId {
        self.theme_id
    }

    pub fn set_theme(&mut self, theme_id: ThemeId) {
        self.theme_id = theme_id;
    }

    pub fn result_visible_rows(&self) -> usize {
        self.result_pane_height.saturating_sub(RESULT_PANE_OVERHEAD) as usize
    }

    pub fn inspector_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(5) as usize
    }

    pub fn explorer_visible_items(&self) -> usize {
        self.explorer_pane_height.saturating_sub(3) as usize
    }

    pub fn connection_list_visible_items(&self) -> usize {
        self.connection_list_pane_height as usize
    }

    pub fn inspector_ddl_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(3) as usize
    }

    pub fn help_visible_rows(&self) -> usize {
        (self.terminal_height as usize * HELP_MODAL_HEIGHT_PERCENT as usize / 100)
            .saturating_sub(MODAL_VERTICAL_BORDER_OVERHEAD)
    }

    pub fn help_max_scroll(&self) -> usize {
        help_content_line_count().saturating_sub(self.help_visible_rows())
    }

    pub fn toggle_focus(&mut self) -> bool {
        if let Some(prev) = self.focus_mode.previous_pane() {
            self.focus_mode = FocusMode::Normal;
            self.focused_pane = prev;
        } else {
            self.focus_mode = FocusMode::focused(self.focused_pane);
            self.focused_pane = FocusedPane::Result;
        }
        true
    }

    pub fn set_explorer_selection(&mut self, index: Option<usize>) {
        if let Some(i) = index {
            self.explorer_scroll_offset = clamp_scroll_offset(
                i,
                self.explorer_scroll_offset,
                self.explorer_visible_items(),
            );
            self.explorer_selected = i;
        } else {
            self.explorer_selected = 0;
            self.explorer_scroll_offset = 0;
        }
    }

    pub fn set_connection_list_selection(&mut self, index: Option<usize>) {
        if let Some(i) = index {
            self.connection_list_scroll_offset = clamp_scroll_offset(
                i,
                self.connection_list_scroll_offset,
                self.connection_list_visible_items(),
            );
            self.connection_list_selected = i;
        } else {
            self.connection_list_selected = 0;
            self.connection_list_scroll_offset = 0;
        }
    }
}

pub fn list_scroll_offset(selected: usize, viewport: usize) -> usize {
    selected.saturating_sub(viewport.saturating_sub(1))
}

pub fn scroll_max_offset(total_items: usize, viewport_size: usize) -> usize {
    total_items.saturating_sub(viewport_size)
}

pub fn text_display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn explorer_content_width_from_inner_width(inner_width: u16) -> usize {
    inner_width.saturating_sub(EXPLORER_HIGHLIGHT_SYMBOL_WIDTH + EXPLORER_SCROLLBAR_RESERVED_WIDTH)
        as usize
}

pub fn explorer_content_width_from_pane_width(pane_width: u16) -> usize {
    explorer_content_width_from_inner_width(pane_width.saturating_sub(EXPLORER_PANEL_BORDER_WIDTH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod constructors {
        use super::*;

        #[test]
        fn default_creates_empty_state() {
            let state = UiState::default();

            assert_eq!(state.focused_pane, FocusedPane::default());
            assert_eq!(state.focus_mode, FocusMode::Normal);
            assert_eq!(state.explorer_selected, 0);
            assert!(state.table_picker.filter_input.content().is_empty());
        }

        #[test]
        fn new_sets_terminal_height() {
            let state = UiState::new();

            assert_eq!(state.terminal_height, 24);
        }
    }

    mod horizontal_scroll_helpers {
        use super::*;

        #[test]
        fn pane_width_excludes_panel_chrome_from_visible_content_width() {
            assert_eq!(explorer_content_width_from_pane_width(20), 15);
        }

        #[test]
        fn inner_width_excludes_list_chrome_from_visible_content_width() {
            assert_eq!(explorer_content_width_from_inner_width(18), 15);
        }

        #[test]
        fn tiny_pane_width_returns_zero_visible_content_width() {
            assert_eq!(explorer_content_width_from_pane_width(4), 0);
        }

        #[test]
        fn larger_content_preserves_remaining_scrollable_range() {
            assert_eq!(scroll_max_offset(30, 15), 15);
        }

        #[test]
        fn fitting_content_returns_zero_max_offset() {
            assert_eq!(scroll_max_offset(10, 15), 0);
        }

        #[test]
        fn double_width_characters_count_as_two_columns() {
            assert_eq!(text_display_width("日本語"), 6);
        }
    }

    mod pane_metrics {
        use super::*;

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
                "max_scroll={max_scroll}, visible={visible}, total={total_rows}"
            );
        }
    }

    mod focus_mode {
        use super::*;

        #[test]
        fn toggle_focus_enters_focus_mode() {
            let mut state = UiState {
                focused_pane: FocusedPane::Explorer,
                ..Default::default()
            };

            let result = state.toggle_focus();

            assert!(result);
            assert!(state.is_focus_mode());
            assert_eq!(state.focused_pane, FocusedPane::Result);
            assert_eq!(
                state.focus_mode.previous_pane(),
                Some(FocusedPane::Explorer)
            );
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
            assert!(!state.is_focus_mode());
            assert_eq!(state.focused_pane, FocusedPane::Inspector);
        }
    }

    mod list_selection {
        use super::*;

        #[test]
        fn set_explorer_selection_with_some_sets_index() {
            let mut state = UiState::default();

            state.set_explorer_selection(Some(5));

            assert_eq!(state.explorer_selected, 5);
        }

        #[test]
        fn set_explorer_selection_with_none_resets_to_zero() {
            let mut state = UiState::default();
            state.set_explorer_selection(Some(10));

            state.set_explorer_selection(None);

            assert_eq!(state.explorer_selected, 0);
        }

        #[test]
        fn set_connection_list_selection_with_some_sets_index() {
            let mut state = UiState::default();

            state.set_connection_list_selection(Some(3));

            assert_eq!(state.connection_list_selected, 3);
        }

        #[test]
        fn set_connection_list_selection_with_none_resets_to_zero() {
            let mut state = UiState::default();
            state.set_connection_list_selection(Some(5));

            state.set_connection_list_selection(None);

            assert_eq!(state.connection_list_selected, 0);
        }
    }

    mod invariants {
        use super::*;

        #[test]
        fn result_overhead_constants_are_consistent() {
            assert_eq!(RESULT_PANE_OVERHEAD, RESULT_INNER_OVERHEAD + 2);
        }
    }

    mod help_scroll {
        use super::*;

        #[test]
        fn help_max_scroll_plus_viewport_equals_content_line_count() {
            let terminal_height: u16 = 24;
            let state = UiState {
                terminal_height,
                ..Default::default()
            };
            let viewport = state.help_visible_rows();

            let max = state.help_max_scroll();

            assert_eq!(
                max + viewport,
                help_content_line_count(),
                "max_scroll({}) + viewport({}) != total_lines({})",
                max,
                viewport,
                help_content_line_count()
            );
        }

        #[test]
        fn help_max_scroll_is_zero_when_terminal_very_tall() {
            let state = UiState {
                terminal_height: 1000,
                ..Default::default()
            };

            let max = state.help_max_scroll();

            assert_eq!(max, 0);
        }

        #[test]
        fn help_visible_rows_matches_modal_layout_height() {
            let state = UiState {
                terminal_height: 24,
                ..Default::default()
            };

            assert_eq!(state.help_visible_rows(), 17);
        }
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
        fn enter_cell_transitions_to_cell_active() {
            let mut sel = ResultSelection::default();

            sel.enter_cell(5, 7);

            assert_eq!(sel.mode(), ResultNavMode::CellActive);
            assert_eq!(sel.row(), Some(5));
            assert_eq!(sel.cell(), Some(7));
        }

        #[test]
        fn move_cell_updates_column_when_active() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(3, 1);

            sel.move_cell(7);

            assert_eq!(sel.mode(), ResultNavMode::CellActive);
            assert_eq!(sel.row(), Some(3));
            assert_eq!(sel.cell(), Some(7));
        }

        #[test]
        fn move_cell_without_selection_is_noop() {
            let mut sel = ResultSelection::default();

            sel.move_cell(5);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn reset_clears_both() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(1, 2);

            sel.reset();

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn enter_cell_replaces_previous_selection() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(0, 3);

            sel.enter_cell(5, 1);

            assert_eq!(sel.mode(), ResultNavMode::CellActive);
            assert_eq!(sel.row(), Some(5));
            assert_eq!(sel.cell(), Some(1));
        }

        #[test]
        fn move_row_preserves_cell() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(0, 3);

            sel.move_row(5);

            assert_eq!(sel.mode(), ResultNavMode::CellActive);
            assert_eq!(sel.row(), Some(5));
            assert_eq!(sel.cell(), Some(3));
        }

        #[test]
        fn move_row_in_scroll_mode_is_noop() {
            let mut sel = ResultSelection::default();

            sel.move_row(7);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
            assert_eq!(sel.row(), None);
        }

        #[test]
        fn clamp_resets_when_zero_rows() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(5, 0);

            sel.clamp(0, 10);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn clamp_resets_when_row_out_of_bounds() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(10, 2);

            sel.clamp(5, 10);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn clamp_caps_cell_to_max_cols() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(0, 9);

            sel.clamp(10, 5);

            assert_eq!(sel.cell(), Some(4));
        }

        #[test]
        fn clamp_resets_when_zero_cols() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(0, 3);

            sel.clamp(10, 0);

            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }

        #[test]
        fn clamp_preserves_valid_selection() {
            let mut sel = ResultSelection::default();
            sel.enter_cell(3, 2);

            sel.clamp(10, 10);

            assert_eq!(sel.row(), Some(3));
            assert_eq!(sel.cell(), Some(2));
        }

        #[test]
        fn accessors_hide_inconsistent_state() {
            let sel = ResultSelection {
                row: Some(1),
                cell: None,
            };

            assert_eq!(sel.row(), None);
            assert_eq!(sel.cell(), None);
            assert_eq!(sel.mode(), ResultNavMode::Scroll);
        }
    }
}
