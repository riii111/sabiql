use crate::app::model::shared::text_input::TextInputState;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellEditState {
    pub row: Option<usize>,
    pub col: Option<usize>,
    pub original_value: String,
    pub input: TextInputState,
}

impl CellEditState {
    pub fn begin(&mut self, row: usize, col: usize, value: String) {
        self.row = Some(row);
        self.col = Some(col);
        self.original_value.clone_from(&value);
        self.input.set_content(value);
    }

    pub fn is_active(&self) -> bool {
        self.row.is_some() && self.col.is_some()
    }

    pub fn has_pending_draft(&self) -> bool {
        self.is_active() && self.input.content() != self.original_value
    }

    pub fn draft_value(&self) -> &str {
        self.input.content()
    }

    pub fn clear(&mut self) {
        self.row = None;
        self.col = None;
        self.original_value.clear();
        self.input.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::action::CursorMove;

    #[test]
    fn begin_with_value_sets_active_state_with_copied_values_returns_expected() {
        let mut state = CellEditState::default();

        state.begin(3, 5, "Alice".to_string());

        assert_eq!(state.row, Some(3));
        assert_eq!(state.col, Some(5));
        assert_eq!(state.original_value, "Alice");
        assert_eq!(state.draft_value(), "Alice");
        assert_eq!(state.input.cursor(), 5); // cursor at end
        assert!(state.is_active());
    }

    #[test]
    fn only_row_selected_yields_inactive_returns_expected() {
        let state = CellEditState {
            row: Some(1),
            col: None,
            original_value: String::new(),
            input: TextInputState::default(),
        };

        assert!(!state.is_active());
    }

    #[test]
    fn only_col_selected_yields_inactive_returns_expected() {
        let state = CellEditState {
            row: None,
            col: Some(1),
            original_value: String::new(),
            input: TextInputState::default(),
        };

        assert!(!state.is_active());
    }

    #[test]
    fn draft_equals_original_yields_false_for_has_pending_draft_returns_expected() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "Alice".to_string());

        assert!(!state.has_pending_draft());
    }

    #[test]
    fn draft_differs_yields_true_for_has_pending_draft_returns_expected() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "Alice".to_string());
        state.input.set_content("Bob".to_string());

        assert!(state.has_pending_draft());
    }

    #[test]
    fn inactive_cell_edit_yields_false_for_has_pending_draft_returns_expected() {
        let state = CellEditState::default();

        assert!(!state.has_pending_draft());
    }

    #[test]
    fn clear_after_begin_resets_all_fields_returns_expected() {
        let mut state = CellEditState::default();
        state.begin(1, 2, "Before".to_string());
        state.input.set_content("After".to_string());

        state.clear();

        assert_eq!(state.row, None);
        assert_eq!(state.col, None);
        assert_eq!(state.original_value, "");
        assert_eq!(state.draft_value(), "");
        assert!(!state.is_active());
    }

    #[test]
    fn cursor_movement_works_through_input_returns_expected() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "hello".to_string());

        state.input.move_cursor(CursorMove::Home);
        assert_eq!(state.input.cursor(), 0);

        state.input.insert_char('X');
        assert_eq!(state.draft_value(), "Xhello");
        assert_eq!(state.input.cursor(), 1);
    }

    #[test]
    fn backspace_at_middle_removes_correct_char_returns_expected() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "abcd".to_string());

        state.input.move_cursor(CursorMove::Left);
        state.input.move_cursor(CursorMove::Left);
        state.input.backspace();

        assert_eq!(state.draft_value(), "acd");
        assert_eq!(state.input.cursor(), 1);
    }

    #[test]
    fn delete_at_cursor_position_returns_expected() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "abcd".to_string());

        state.input.move_cursor(CursorMove::Home);
        state.input.delete();

        assert_eq!(state.draft_value(), "bcd");
        assert_eq!(state.input.cursor(), 0);
    }
}
