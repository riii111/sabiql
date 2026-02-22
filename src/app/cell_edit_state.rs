#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellEditState {
    pub row: Option<usize>,
    pub col: Option<usize>,
    pub original_value: String,
    pub draft_value: String,
}

impl CellEditState {
    pub fn begin(&mut self, row: usize, col: usize, value: String) {
        self.row = Some(row);
        self.col = Some(col);
        self.original_value = value.clone();
        self.draft_value = value;
    }

    pub fn is_active(&self) -> bool {
        self.row.is_some() && self.col.is_some()
    }

    pub fn clear(&mut self) {
        self.row = None;
        self.col = None;
        self.original_value.clear();
        self.draft_value.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_with_value_sets_active_state_with_copied_values() {
        let mut state = CellEditState::default();

        state.begin(3, 5, "Alice".to_string());

        assert_eq!(state.row, Some(3));
        assert_eq!(state.col, Some(5));
        assert_eq!(state.original_value, "Alice");
        assert_eq!(state.draft_value, "Alice");
        assert!(state.is_active());
    }

    #[test]
    fn only_row_selected_returns_inactive() {
        let state = CellEditState {
            row: Some(1),
            col: None,
            original_value: String::new(),
            draft_value: String::new(),
        };

        assert!(!state.is_active());
    }

    #[test]
    fn only_col_selected_returns_inactive() {
        let state = CellEditState {
            row: None,
            col: Some(1),
            original_value: String::new(),
            draft_value: String::new(),
        };

        assert!(!state.is_active());
    }

    #[test]
    fn clear_after_begin_resets_all_fields() {
        let mut state = CellEditState::default();
        state.begin(1, 2, "Before".to_string());
        state.draft_value = "After".to_string();

        state.clear();

        assert_eq!(state.row, None);
        assert_eq!(state.col, None);
        assert_eq!(state.original_value, "");
        assert_eq!(state.draft_value, "");
        assert!(!state.is_active());
    }
}
