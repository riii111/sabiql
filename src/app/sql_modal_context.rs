use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlModalStatus {
    #[default]
    Editing,
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Default)]
pub struct SqlModalContext {
    pub content: String,
    pub cursor: usize,
    pub status: SqlModalStatus,
    pub completion: CompletionState,
    pub completion_debounce: Option<Instant>,
    pub prefetch_queue: VecDeque<String>,
    pub prefetching_tables: HashSet<String>,
    pub failed_prefetch_tables: HashMap<String, (Instant, String)>,
    pub prefetch_started: bool,
}

impl SqlModalContext {
    pub fn reset_prefetch(&mut self) {
        self.prefetch_started = false;
        self.prefetch_queue.clear();
        self.prefetching_tables.clear();
        self.failed_prefetch_tables.clear();
    }

    pub fn clear_content(&mut self) {
        self.content.clear();
        self.cursor = 0;
        self.completion.visible = false;
        self.completion.candidates.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_creates_empty_context() {
        let ctx = SqlModalContext::default();

        assert!(ctx.content.is_empty());
        assert_eq!(ctx.cursor, 0);
        assert_eq!(ctx.status, SqlModalStatus::Editing);
        assert!(!ctx.completion.visible);
        assert!(!ctx.prefetch_started);
    }

    #[test]
    fn reset_prefetch_clears_all_prefetch_state() {
        let mut ctx = SqlModalContext::default();
        ctx.prefetch_started = true;
        ctx.prefetch_queue.push_back("public.users".to_string());
        ctx.prefetching_tables.insert("public.posts".to_string());
        ctx.failed_prefetch_tables.insert(
            "public.failed".to_string(),
            (Instant::now(), "error".to_string()),
        );

        ctx.reset_prefetch();

        assert!(!ctx.prefetch_started);
        assert!(ctx.prefetch_queue.is_empty());
        assert!(ctx.prefetching_tables.is_empty());
        assert!(ctx.failed_prefetch_tables.is_empty());
    }

    #[test]
    fn clear_content_resets_editor_state() {
        let mut ctx = SqlModalContext::default();
        ctx.content = "SELECT * FROM users".to_string();
        ctx.cursor = 10;
        ctx.completion.visible = true;
        ctx.completion.candidates.push(CompletionCandidate {
            text: "test".to_string(),
            kind: CompletionKind::Table,
            score: 100,
        });

        ctx.clear_content();

        assert!(ctx.content.is_empty());
        assert_eq!(ctx.cursor, 0);
        assert!(!ctx.completion.visible);
        assert!(ctx.completion.candidates.is_empty());
    }

    #[test]
    fn recent_columns_vec_returns_clone() {
        let mut state = CompletionState::default();
        state.recent_columns.push_back("col1".to_string());
        state.recent_columns.push_back("col2".to_string());

        let vec = state.recent_columns_vec();

        assert_eq!(vec, vec!["col1".to_string(), "col2".to_string()]);
    }
}
