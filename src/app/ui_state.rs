use ratatui::widgets::ListState;

use super::focused_pane::FocusedPane;
use super::input_mode::InputMode;
use super::inspector_tab::InspectorTab;
use crate::ui::components::viewport_columns::ViewportPlan;

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub focused_pane: FocusedPane,
    pub focus_mode: bool,
    pub focus_mode_prev_pane: Option<FocusedPane>,
    pub input_mode: InputMode,

    pub explorer_selected: usize,
    pub explorer_horizontal_offset: usize,
    pub explorer_list_state: ListState,

    pub picker_selected: usize,
    pub picker_list_state: ListState,
    pub filter_input: String,

    pub inspector_tab: InspectorTab,
    pub inspector_scroll_offset: usize,
    pub inspector_horizontal_offset: usize,
    pub inspector_viewport_plan: ViewportPlan,
    pub inspector_pane_height: u16,

    pub result_scroll_offset: usize,
    pub result_horizontal_offset: usize,
    pub result_viewport_plan: ViewportPlan,
    pub result_pane_height: u16,

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
        self.result_pane_height.saturating_sub(3) as usize
    }

    pub fn inspector_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(5) as usize
    }

    pub fn inspector_ddl_visible_rows(&self) -> usize {
        self.inspector_pane_height.saturating_sub(3) as usize
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
    #[case(10, 7)]
    #[case(15, 12)]
    #[case(20, 17)]
    #[case(30, 27)]
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
}
