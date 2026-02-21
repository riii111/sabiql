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
