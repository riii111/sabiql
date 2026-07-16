use crate::model::shared::cursor::CursorMove;
use crate::model::shared::text_input::{TextInputEditing, TextInputState};
use crate::update::action::TextKillDirection;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellEditState {
    row: Option<usize>,
    col: Option<usize>,
    original_value: String,
    input: TextInputState,
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

    pub fn row(&self) -> Option<usize> {
        self.row
    }

    pub fn col(&self) -> Option<usize> {
        self.col
    }

    pub fn original_value(&self) -> &str {
        &self.original_value
    }

    pub fn input(&self) -> &TextInputState {
        &self.input
    }

    pub fn insert_char(&mut self, ch: char) {
        self.input.insert_char(ch);
    }

    pub fn insert_str(&mut self, text: &str) {
        self.input.insert_str(text);
    }

    pub fn backspace(&mut self) {
        self.input.backspace();
    }

    pub fn delete(&mut self) {
        self.input.delete();
    }

    pub fn kill(&mut self, direction: TextKillDirection) -> String {
        self.input.kill(direction)
    }

    pub fn yank(&mut self, text: &str) {
        self.input.yank(text);
    }

    pub fn move_cursor(&mut self, direction: CursorMove) {
        self.input.move_cursor(direction);
    }

    pub fn replace_draft(&mut self, content: String) {
        self.input.set_content(content);
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.input.set_cursor(cursor);
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

    #[test]
    fn begin_with_value_sets_active_state_with_copied_values() {
        let mut state = CellEditState::default();

        state.begin(3, 5, "Alice".to_string());

        assert_eq!(state.row(), Some(3));
        assert_eq!(state.col(), Some(5));
        assert_eq!(state.original_value(), "Alice");
        assert_eq!(state.draft_value(), "Alice");
        assert_eq!(state.input.cursor(), 5); // cursor at end
        assert!(state.is_active());
    }

    #[test]
    fn is_active_requires_both_row_and_col() {
        assert!(!CellEditState::default().is_active());

        let mut state = CellEditState::default();
        state.begin(1, 2, "Alice".to_string());
        assert!(state.is_active());
    }

    #[test]
    fn has_pending_draft_returns_false_when_draft_equals_original() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "Alice".to_string());

        assert!(!state.has_pending_draft());
    }

    #[test]
    fn has_pending_draft_returns_true_when_draft_differs() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "Alice".to_string());
        state.replace_draft("Bob".to_string());

        assert!(state.has_pending_draft());
    }

    #[test]
    fn has_pending_draft_returns_false_when_not_active() {
        let state = CellEditState::default();

        assert!(!state.has_pending_draft());
    }

    #[test]
    fn clear_after_begin_resets_all_fields() {
        let mut state = CellEditState::default();
        state.begin(1, 2, "Before".to_string());
        state.replace_draft("After".to_string());

        state.clear();

        assert_eq!(state.row(), None);
        assert_eq!(state.col(), None);
        assert_eq!(state.original_value(), "");
        assert_eq!(state.draft_value(), "");
        assert!(!state.is_active());
    }

    #[test]
    fn cursor_movement_works_through_input() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "hello".to_string());

        state.move_cursor(CursorMove::Home);
        assert_eq!(state.input().cursor(), 0);

        state.insert_char('X');
        assert_eq!(state.draft_value(), "Xhello");
        assert_eq!(state.input().cursor(), 1);
    }

    #[test]
    fn backspace_at_middle_removes_correct_char() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "abcd".to_string());

        state.move_cursor(CursorMove::Left);
        state.move_cursor(CursorMove::Left);
        state.backspace();

        assert_eq!(state.draft_value(), "acd");
        assert_eq!(state.input().cursor(), 1);
    }

    #[test]
    fn delete_at_cursor_position() {
        let mut state = CellEditState::default();
        state.begin(0, 0, "abcd".to_string());

        state.move_cursor(CursorMove::Home);
        state.delete();

        assert_eq!(state.draft_value(), "bcd");
        assert_eq!(state.input().cursor(), 0);
    }
}
