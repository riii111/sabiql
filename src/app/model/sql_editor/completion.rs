use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub text: String,
    pub kind: CompletionKind,
    pub score: i32,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionState {
    pub visible: bool,
    pub candidates: Vec<CompletionCandidate>,
    pub selected_index: usize,
    pub trigger_position: usize,
    pub recent_columns: VecDeque<String>,
}

impl CompletionState {
    pub fn recent_columns_vec(&self) -> Vec<String> {
        self.recent_columns.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_columns_vec_returns_clone() {
        let mut state = CompletionState::default();
        state.recent_columns.push_back("col1".to_string());
        state.recent_columns.push_back("col2".to_string());

        let vec = state.recent_columns_vec();

        assert_eq!(vec, vec!["col1".to_string(), "col2".to_string()]);
    }
}
