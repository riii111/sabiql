use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::app::text_input::TextInputState;
use crate::app::write_guardrails::AdhocRiskDecision;
use crate::domain::CommandTag;

/// Sized so that prompt + input + checkmark fits within the 80-col modal inner width (~62 cols).
pub const HIGH_RISK_INPUT_VISIBLE_WIDTH: usize = 30;

#[derive(Debug, Clone)]
pub struct FailedPrefetchEntry {
    pub failed_at: Instant,
    pub error: String,
    pub retry_count: u32,
}

#[derive(Debug, Clone)]
pub struct AdhocSuccessSnapshot {
    pub command_tag: Option<CommandTag>,
    pub row_count: usize,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SqlModalStatus {
    #[default]
    Editing,
    /// Awaiting explicit Enter confirmation before executing a write statement.
    /// Holds the risk assessment so the UI can show an appropriate warning.
    Confirming(AdhocRiskDecision),
    /// HIGH risk confirmation requiring the user to type the target table name.
    /// `target_name: None` means extraction failed — execution is permanently blocked.
    ConfirmingHigh {
        decision: AdhocRiskDecision,
        input: TextInputState,
        target_name: Option<String>,
    },
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

#[derive(Debug, Clone, Default)]
pub struct SqlModalContext {
    pub content: String,
    pub cursor: usize,
    pub status: SqlModalStatus,
    pub last_adhoc_success: Option<AdhocSuccessSnapshot>,
    pub last_adhoc_error: Option<String>,
    pub completion: CompletionState,
    pub completion_debounce: Option<Instant>,
    pub prefetch_queue: VecDeque<String>,
    pub prefetching_tables: HashSet<String>,
    pub failed_prefetch_tables: HashMap<String, FailedPrefetchEntry>,
    pub prefetch_started: bool,
}

#[cfg(test)]
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
        let mut ctx = SqlModalContext {
            prefetch_started: true,
            ..Default::default()
        };
        ctx.prefetch_queue.push_back("public.users".to_string());
        ctx.prefetching_tables.insert("public.posts".to_string());
        ctx.failed_prefetch_tables.insert(
            "public.failed".to_string(),
            FailedPrefetchEntry {
                failed_at: Instant::now(),
                error: "error".to_string(),
                retry_count: 0,
            },
        );

        ctx.reset_prefetch();

        assert!(!ctx.prefetch_started);
        assert!(ctx.prefetch_queue.is_empty());
        assert!(ctx.prefetching_tables.is_empty());
        assert!(ctx.failed_prefetch_tables.is_empty());
    }

    #[test]
    fn clear_content_resets_editor_state() {
        let mut ctx = SqlModalContext {
            content: "SELECT * FROM users".to_string(),
            cursor: 10,
            ..Default::default()
        };
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
    fn confirming_high_with_target_name() {
        use crate::app::write_guardrails::RiskLevel;

        let status = SqlModalStatus::ConfirmingHigh {
            decision: AdhocRiskDecision {
                risk_level: RiskLevel::High,
                label: "DROP",
            },
            input: TextInputState::default(),
            target_name: Some("users".to_string()),
        };

        assert!(matches!(
            status,
            SqlModalStatus::ConfirmingHigh {
                target_name: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn confirming_high_without_target_name() {
        use crate::app::write_guardrails::RiskLevel;

        let status = SqlModalStatus::ConfirmingHigh {
            decision: AdhocRiskDecision {
                risk_level: RiskLevel::High,
                label: "SQL",
            },
            input: TextInputState::default(),
            target_name: None,
        };

        assert!(matches!(
            status,
            SqlModalStatus::ConfirmingHigh {
                target_name: None,
                ..
            }
        ));
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
