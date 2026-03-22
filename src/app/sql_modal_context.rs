use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::app::text_input::TextInputState;
use crate::app::write_guardrails::AdhocRiskDecision;
use crate::domain::CommandTag;

// Sized so that prompt + input + checkmark fits within the 80-col modal inner width (~62 cols).
pub const HIGH_RISK_INPUT_VISIBLE_WIDTH: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlModalTab {
    #[default]
    Sql,
    Plan,
    Compare,
}

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
    Normal,
    Editing,
    Confirming(AdhocRiskDecision),
    // HIGH risk confirmation requiring the user to type the target table name.
    // `target_name: None` means extraction failed — execution is permanently blocked.
    ConfirmingHigh {
        decision: AdhocRiskDecision,
        input: TextInputState,
        target_name: Option<String>,
    },
    ConfirmingAnalyze {
        query: String,
        is_dml: bool,
    },
    ConfirmingAnalyzeHigh {
        query: String,
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
    status: SqlModalStatus,
    last_adhoc_success: Option<AdhocSuccessSnapshot>,
    last_adhoc_error: Option<String>,
    pub completion: CompletionState,
    pub completion_debounce: Option<Instant>,
    pub prefetch_queue: VecDeque<String>,
    pub prefetching_tables: HashSet<String>,
    pub failed_prefetch_tables: HashMap<String, FailedPrefetchEntry>,
    prefetch_started: bool,
    pub active_tab: SqlModalTab,
}

impl SqlModalContext {
    // ── Prefetch lifecycle ──────────────────────────────────────────

    pub fn reset_prefetch(&mut self) {
        self.prefetch_started = false;
        self.prefetch_queue.clear();
        self.prefetching_tables.clear();
        self.failed_prefetch_tables.clear();
    }

    // Preserves `prefetching_tables` so in-flight requests drain naturally.
    pub fn begin_prefetch(&mut self) {
        self.prefetch_started = true;
        self.prefetch_queue.clear();
        self.failed_prefetch_tables.clear();
    }

    pub fn invalidate_prefetch(&mut self) {
        self.prefetch_started = false;
    }

    pub fn is_prefetch_started(&self) -> bool {
        self.prefetch_started
    }

    // ── Adhoc status ────────────────────────────────────────────────

    pub fn mark_adhoc_error(&mut self, error: String) {
        self.status = SqlModalStatus::Error;
        self.last_adhoc_error = Some(error);
        self.last_adhoc_success = None;
    }

    pub fn mark_adhoc_success(&mut self, snapshot: AdhocSuccessSnapshot) {
        self.status = SqlModalStatus::Success;
        self.last_adhoc_success = Some(snapshot);
        self.last_adhoc_error = None;
    }

    pub fn set_status(&mut self, status: SqlModalStatus) {
        debug_assert!(
            !matches!(status, SqlModalStatus::Error | SqlModalStatus::Success),
            "adhoc completion must use mark_adhoc_error/mark_adhoc_success to maintain mutual exclusion"
        );
        self.status = status;
    }

    pub fn status(&self) -> &SqlModalStatus {
        &self.status
    }

    pub fn last_adhoc_error(&self) -> Option<&str> {
        self.last_adhoc_error.as_deref()
    }

    pub fn last_adhoc_success(&self) -> Option<&AdhocSuccessSnapshot> {
        self.last_adhoc_success.as_ref()
    }

    pub fn confirming_high_input_mut(&mut self) -> Option<&mut TextInputState> {
        if let SqlModalStatus::ConfirmingHigh { ref mut input, .. } = self.status {
            Some(input)
        } else {
            None
        }
    }

    pub fn confirming_analyze_high_input_mut(&mut self) -> Option<&mut TextInputState> {
        if let SqlModalStatus::ConfirmingAnalyzeHigh { ref mut input, .. } = self.status {
            Some(input)
        } else {
            None
        }
    }
}

#[cfg(test)]
impl SqlModalContext {
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
        assert_eq!(ctx.status, SqlModalStatus::Normal);
        assert!(!ctx.completion.visible);
        assert!(!ctx.is_prefetch_started());
    }

    #[test]
    fn reset_prefetch_clears_all_prefetch_state() {
        let mut ctx = SqlModalContext::default();
        ctx.begin_prefetch();
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

        assert!(!ctx.is_prefetch_started());
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
