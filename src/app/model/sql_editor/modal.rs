use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::domain::CommandTag;
use crate::model::shared::multi_line_input::MultiLineInputState;
use crate::model::shared::text_input::TextInputState;
use crate::policy::write::write_guardrails::AdhocRiskDecision;

use super::completion::{CompletionCandidate, CompletionState};

// Sized so that prompt + input + checkmark fits within the 80-col modal inner width (~62 cols).
pub const HIGH_RISK_INPUT_VISIBLE_WIDTH: usize = 30;
pub const SQL_MODAL_HEIGHT_PERCENT: u16 = 60;
// border top/bottom (2) + separator (1) + status row (1)
pub const SQL_MODAL_CHROME_LINES: usize = 4;
pub const SQL_MODAL_VISIBLE_ROWS_FALLBACK: usize = 8;

pub fn sql_modal_visible_rows(terminal_height: u16) -> usize {
    if terminal_height == 0 {
        return SQL_MODAL_VISIBLE_ROWS_FALLBACK;
    }

    (terminal_height as usize * SQL_MODAL_HEIGHT_PERCENT as usize / 100)
        .saturating_sub(SQL_MODAL_CHROME_LINES)
        .max(1)
}

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
    // HIGH risk confirmation requiring the user to type the target object name.
    ConfirmingHigh {
        decision: AdhocRiskDecision,
        input: TextInputState,
        target_name: Option<String>,
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

#[derive(Debug, Clone, Default)]
pub struct SqlModalContext {
    pub editor: MultiLineInputState,
    status: SqlModalStatus,
    last_adhoc_success: Option<AdhocSuccessSnapshot>,
    last_adhoc_error: Option<String>,
    completion: CompletionState,
    completion_debounce: Option<Instant>,
    prefetch_queue: VecDeque<String>,
    prefetching_tables: HashSet<String>,
    failed_prefetch_tables: HashMap<String, FailedPrefetchEntry>,
    prefetch_started: bool,
    active_tab: SqlModalTab,
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

    pub fn prefetch_queue(&self) -> &VecDeque<String> {
        &self.prefetch_queue
    }

    pub fn prefetching_tables(&self) -> &HashSet<String> {
        &self.prefetching_tables
    }

    pub fn failed_prefetch_tables(&self) -> &HashMap<String, FailedPrefetchEntry> {
        &self.failed_prefetch_tables
    }

    pub fn enqueue_prefetch(&mut self, table: String) {
        self.prefetch_queue.push_back(table);
    }

    pub fn pop_prefetch(&mut self) -> Option<String> {
        self.prefetch_queue.pop_front()
    }

    pub fn mark_prefetching(&mut self, table: String) {
        self.prefetching_tables.insert(table);
    }

    pub fn finish_prefetch(&mut self, table: &str) {
        self.prefetching_tables.remove(table);
        self.failed_prefetch_tables.remove(table);
    }

    pub fn record_prefetch_failure(&mut self, table: String, entry: FailedPrefetchEntry) {
        self.prefetching_tables.remove(&table);
        self.failed_prefetch_tables.insert(table, entry);
    }

    // ── Adhoc status ────────────────────────────────────────────────

    pub fn begin_adhoc_running(&mut self) {
        self.status = SqlModalStatus::Running;
        self.dismiss_completion();
    }

    pub fn finish_adhoc_error(&mut self, error: String) {
        self.status = SqlModalStatus::Error;
        self.last_adhoc_error = Some(error);
        self.last_adhoc_success = None;
    }

    pub fn finish_adhoc_success(&mut self, snapshot: AdhocSuccessSnapshot) {
        self.status = SqlModalStatus::Success;
        self.last_adhoc_success = Some(snapshot);
        self.last_adhoc_error = None;
    }

    pub fn begin_confirming_high(
        &mut self,
        decision: AdhocRiskDecision,
        target_name: Option<String>,
    ) {
        self.status = SqlModalStatus::ConfirmingHigh {
            decision,
            input: TextInputState::default(),
            target_name,
        };
        self.dismiss_completion();
    }

    pub fn begin_confirming_analyze_high(&mut self, query: String, target_name: Option<String>) {
        self.status = SqlModalStatus::ConfirmingAnalyzeHigh {
            query,
            input: TextInputState::default(),
            target_name,
        };
        self.active_tab = SqlModalTab::Plan;
        self.dismiss_completion();
    }

    pub fn cancel_confirmation(&mut self) {
        if matches!(
            self.status,
            SqlModalStatus::ConfirmingHigh { .. } | SqlModalStatus::ConfirmingAnalyzeHigh { .. }
        ) {
            self.status = SqlModalStatus::Normal;
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn set_status_for_test(&mut self, status: SqlModalStatus) {
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

    pub fn active_tab(&self) -> SqlModalTab {
        self.active_tab
    }

    pub fn set_active_tab(&mut self, tab: SqlModalTab) {
        self.active_tab = tab;
    }

    pub fn open_sql_tab(&mut self) {
        self.status = SqlModalStatus::Normal;
        self.active_tab = SqlModalTab::Sql;
        self.reset_completion();
    }

    pub fn cleanup_on_close(&mut self) {
        self.dismiss_completion();
    }

    pub fn enter_editing(&mut self) {
        self.status = SqlModalStatus::Editing;
    }

    pub fn enter_normal(&mut self) {
        self.status = SqlModalStatus::Normal;
        self.dismiss_completion();
    }

    pub fn load_query_from_history(&mut self, query: String) {
        self.editor.set_content(query);
        self.open_sql_tab();
    }

    pub fn load_query_for_editing(&mut self, query: String) {
        self.editor.set_content(query);
        self.status = SqlModalStatus::Editing;
        self.active_tab = SqlModalTab::Sql;
        self.reset_completion();
    }

    pub fn completion(&self) -> &CompletionState {
        &self.completion
    }

    pub fn completion_debounce(&self) -> Option<Instant> {
        self.completion_debounce
    }

    pub fn schedule_completion(&mut self, debounce_until: Instant) {
        self.completion_debounce = Some(debounce_until);
    }

    pub fn schedule_completion_after_dismiss(&mut self, debounce_until: Instant) {
        self.completion.visible = false;
        self.schedule_completion(debounce_until);
    }

    pub fn consume_completion_debounce(&mut self) -> Option<Instant> {
        self.completion_debounce.take()
    }

    pub fn dismiss_completion(&mut self) {
        self.completion.visible = false;
        self.completion_debounce = None;
    }

    pub fn reset_completion(&mut self) {
        self.completion.visible = false;
        self.completion.candidates.clear();
        self.completion.selected_index = 0;
        self.completion_debounce = None;
    }

    pub fn apply_completion_update(
        &mut self,
        candidates: &[CompletionCandidate],
        trigger_position: usize,
        visible: bool,
    ) {
        self.completion.candidates.clear();
        self.completion.candidates.extend_from_slice(candidates);
        self.completion.trigger_position = trigger_position;
        self.completion.visible = visible;
        self.completion.selected_index = 0;
    }

    pub fn completion_next(&mut self) {
        if self.completion.candidates.is_empty() {
            return;
        }
        let max = self.completion.candidates.len() - 1;
        self.completion.selected_index = if self.completion.selected_index >= max {
            0
        } else {
            self.completion.selected_index + 1
        };
    }

    pub fn completion_prev(&mut self) {
        if self.completion.candidates.is_empty() {
            return;
        }
        let max = self.completion.candidates.len() - 1;
        self.completion.selected_index = if self.completion.selected_index == 0 {
            max
        } else {
            self.completion.selected_index - 1
        };
    }

    pub fn selected_completion_replacement(&self) -> Option<(usize, String)> {
        if !self.completion.visible || self.completion.candidates.is_empty() {
            return None;
        }
        self.completion
            .candidates
            .get(self.completion.selected_index)
            .map(|candidate| (self.completion.trigger_position, candidate.text.clone()))
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

impl SqlModalContext {
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn clear_content(&mut self) {
        self.editor.clear();
        self.reset_completion();
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn set_completion_for_test(&mut self, completion: CompletionState) {
        self.completion = completion;
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn completion_mut_for_test(&mut self) -> &mut CompletionState {
        &mut self.completion
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn set_completion_debounce_for_test(&mut self, debounce: Option<Instant>) {
        self.completion_debounce = debounce;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::text_input::TextInputLike;
    use crate::model::sql_editor::completion::{CompletionCandidate, CompletionKind};

    fn candidate(text: &str) -> CompletionCandidate {
        CompletionCandidate {
            text: text.to_string(),
            kind: CompletionKind::Keyword,
            score: 1,
        }
    }

    mod lifecycle {
        use super::*;

        #[test]
        fn default_creates_empty_context() {
            let ctx = SqlModalContext::default();

            assert!(ctx.editor.content().is_empty());
            assert_eq!(ctx.editor.cursor(), 0);
            assert_eq!(ctx.status, SqlModalStatus::Normal);
            assert!(!ctx.completion.visible);
            assert!(!ctx.is_prefetch_started());
        }

        #[test]
        fn clear_content_resets_editor_state() {
            let mut ctx = SqlModalContext::default();
            ctx.editor.set_content("SELECT * FROM users".to_string());
            ctx.completion.visible = true;
            ctx.completion.candidates.push(CompletionCandidate {
                text: "test".to_string(),
                kind: CompletionKind::Table,
                score: 100,
            });

            ctx.clear_content();

            assert!(ctx.editor.content().is_empty());
            assert_eq!(ctx.editor.cursor(), 0);
            assert!(!ctx.completion.visible);
            assert!(ctx.completion.candidates.is_empty());
        }
    }

    mod prefetch {
        use super::*;

        #[test]
        fn reset_clears_all_state() {
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
    }

    mod confirmation {
        use super::*;
        use crate::policy::write::write_guardrails::RiskLevel;

        #[test]
        fn high_status_keeps_target_name() {
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
        fn high_status_allows_missing_target_name() {
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
        fn cancel_only_resets_confirmation_status() {
            let mut ctx = SqlModalContext::default();
            ctx.cancel_confirmation();
            assert_eq!(ctx.status, SqlModalStatus::Normal);

            ctx.begin_adhoc_running();
            ctx.cancel_confirmation();
            assert_eq!(ctx.status, SqlModalStatus::Running);

            ctx.begin_confirming_high(
                AdhocRiskDecision {
                    risk_level: RiskLevel::High,
                    label: "DROP",
                },
                Some("users".to_string()),
            );
            ctx.cancel_confirmation();
            assert_eq!(ctx.status, SqlModalStatus::Normal);
        }
    }

    mod completion {
        use super::*;

        #[test]
        fn schedule_preserves_popup_visibility() {
            let mut ctx = SqlModalContext::default();
            let debounce_until = Instant::now();
            ctx.completion.visible = true;

            ctx.schedule_completion(debounce_until);

            assert!(ctx.completion.visible);
            assert_eq!(ctx.completion_debounce, Some(debounce_until));
        }

        #[test]
        fn schedule_after_dismiss_hides_popup() {
            let mut ctx = SqlModalContext::default();
            let debounce_until = Instant::now();
            ctx.completion.visible = true;

            ctx.schedule_completion_after_dismiss(debounce_until);

            assert!(!ctx.completion.visible);
            assert_eq!(ctx.completion_debounce, Some(debounce_until));
        }

        #[test]
        fn navigation_wraps_selection() {
            let mut ctx = SqlModalContext::default();
            ctx.apply_completion_update(&[candidate("a"), candidate("b")], 0, true);

            ctx.completion_prev();
            assert_eq!(ctx.completion.selected_index, 1);

            ctx.completion_next();
            assert_eq!(ctx.completion.selected_index, 0);
        }

        #[test]
        fn selected_replacement_returns_trigger_and_text() {
            let mut ctx = SqlModalContext::default();
            ctx.apply_completion_update(
                &[CompletionCandidate {
                    text: "users".to_string(),
                    kind: CompletionKind::Table,
                    score: 1,
                }],
                7,
                true,
            );

            assert_eq!(
                ctx.selected_completion_replacement(),
                Some((7, "users".to_string()))
            );
        }
    }

    mod adhoc_status {
        use super::*;

        #[test]
        fn finish_statuses_clear_opposite_snapshot() {
            let mut ctx = SqlModalContext::default();

            ctx.finish_adhoc_success(AdhocSuccessSnapshot {
                command_tag: None,
                row_count: 1,
                execution_time_ms: 10,
            });
            assert!(ctx.last_adhoc_success().is_some());
            assert!(ctx.last_adhoc_error().is_none());

            ctx.finish_adhoc_error("syntax error".to_string());

            assert!(ctx.last_adhoc_success().is_none());
            assert_eq!(ctx.last_adhoc_error(), Some("syntax error"));
        }
    }

    mod visible_rows {
        use super::*;

        #[test]
        fn uses_fallback_when_terminal_height_is_zero() {
            assert_eq!(sql_modal_visible_rows(0), SQL_MODAL_VISIBLE_ROWS_FALLBACK);
        }

        #[test]
        fn clamps_to_one_for_small_terminal() {
            assert_eq!(sql_modal_visible_rows(1), 1);
            assert_eq!(sql_modal_visible_rows(8), 1);
        }
    }
}
