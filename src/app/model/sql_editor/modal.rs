use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::domain::CommandTag;
use crate::model::shared::async_run::AsyncRun;
use crate::model::shared::multi_line_input::MultiLineInputState;
use crate::model::shared::text_input::{TextInputLike, TextInputState};
use crate::policy::write::sql_risk::AcknowledgeReason;
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
    // When no target name can be extracted, ConfirmingRisk is used instead.
    ConfirmingHigh {
        decision: AdhocRiskDecision,
        input: TextInputState,
        target_name: String,
    },
    ConfirmingAnalyzeHigh {
        query: String,
        input: TextInputState,
        target_name: String,
    },
    ConfirmingRisk {
        reason: AcknowledgeReason,
        label: String,
    },
    ConfirmingAnalyzeRisk {
        query: String,
        reason: AcknowledgeReason,
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
    pub prefetch_queue: VecDeque<String>,
    pub prefetching_tables: HashSet<String>,
    pub failed_prefetch_tables: HashMap<String, FailedPrefetchEntry>,
    prefetch_started: bool,
    prefetch_run: AsyncRun,
    active_tab: SqlModalTab,
}

impl SqlModalContext {
    // ── Prefetch lifecycle ──────────────────────────────────────────

    pub fn reset_prefetch(&mut self) {
        self.prefetch_started = false;
        self.prefetch_queue.clear();
        self.prefetching_tables.clear();
        self.failed_prefetch_tables.clear();
        self.prefetch_run.clear_active();
    }

    // Preserves `prefetching_tables` so in-flight requests drain naturally.
    #[must_use]
    pub fn begin_prefetch(&mut self) -> u64 {
        self.prefetch_started = true;
        self.prefetch_queue.clear();
        self.failed_prefetch_tables.clear();
        self.prefetch_run.begin()
    }

    pub fn invalidate_prefetch(&mut self) {
        self.prefetch_started = false;
        self.prefetching_tables.clear();
        self.prefetch_run.clear_active();
    }

    pub fn is_prefetch_started(&self) -> bool {
        self.prefetch_started
    }

    pub fn has_pending_prefetch(&self) -> bool {
        !self.prefetch_queue.is_empty()
    }

    pub fn is_prefetch_queued(&self, table: &str) -> bool {
        self.prefetch_queue.iter().any(|queued| queued == table)
    }

    pub fn is_table_prefetching(&self, table: &str) -> bool {
        self.prefetching_tables.contains(table)
    }

    pub fn prefetch_in_flight_count(&self) -> usize {
        self.prefetching_tables.len()
    }

    pub fn failed_prefetch(&self, table: &str) -> Option<&FailedPrefetchEntry> {
        self.failed_prefetch_tables.get(table)
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn prefetch_queue(&self) -> &VecDeque<String> {
        &self.prefetch_queue
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn prefetching_tables(&self) -> &HashSet<String> {
        &self.prefetching_tables
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn failed_prefetch_tables(&self) -> &HashMap<String, FailedPrefetchEntry> {
        &self.failed_prefetch_tables
    }

    pub fn queue_table_prefetch(&mut self, table: String) {
        if self.prefetching_tables.contains(&table) || self.is_prefetch_queued(&table) {
            return;
        }
        self.prefetch_queue.push_back(table);
    }

    pub fn defer_table_prefetch(&mut self, table: String) {
        if self.prefetching_tables.contains(&table) || self.is_prefetch_queued(&table) {
            return;
        }
        self.prefetch_queue.push_front(table);
    }

    pub fn take_next_prefetch(&mut self) -> Option<String> {
        self.prefetch_queue.pop_front()
    }

    pub fn start_table_prefetch(&mut self, table: String) {
        self.prefetch_queue.retain(|queued| queued != &table);
        self.prefetching_tables.insert(table);
    }

    pub fn complete_table_prefetch(&mut self, table: &str) {
        self.prefetch_queue.retain(|queued| queued != table);
        self.prefetching_tables.remove(table);
        self.failed_prefetch_tables.remove(table);
    }

    pub fn fail_table_prefetch(&mut self, table: String, entry: FailedPrefetchEntry) {
        self.prefetch_queue.retain(|queued| queued != &table);
        self.prefetching_tables.remove(&table);
        self.failed_prefetch_tables.insert(table, entry);
    }

    pub fn retry_table_prefetch(&mut self, table: String, entry: FailedPrefetchEntry) {
        self.fail_table_prefetch(table.clone(), entry);
        self.queue_table_prefetch(table);
    }

    pub fn active_prefetch_run_id(&self) -> Option<u64> {
        self.prefetch_run.active_id()
    }

    pub fn is_current_prefetch_run(&self, run_id: u64) -> bool {
        self.prefetch_run.is_current(run_id)
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

    pub fn begin_confirming_high(&mut self, decision: AdhocRiskDecision, target_name: String) {
        self.status = SqlModalStatus::ConfirmingHigh {
            decision,
            input: TextInputState::default(),
            target_name,
        };
        self.dismiss_completion();
    }

    pub fn begin_confirming_analyze_high(&mut self, query: String, target_name: String) {
        self.status = SqlModalStatus::ConfirmingAnalyzeHigh {
            query,
            input: TextInputState::default(),
            target_name,
        };
        self.active_tab = SqlModalTab::Plan;
        self.dismiss_completion();
    }

    pub fn begin_confirming_risk(&mut self, reason: AcknowledgeReason, label: String) {
        self.status = SqlModalStatus::ConfirmingRisk { reason, label };
        self.dismiss_completion();
    }

    pub fn begin_confirming_analyze_risk(&mut self, query: String, reason: AcknowledgeReason) {
        self.status = SqlModalStatus::ConfirmingAnalyzeRisk { query, reason };
        self.active_tab = SqlModalTab::Plan;
        self.dismiss_completion();
    }

    pub fn cancel_confirmation(&mut self) {
        if matches!(
            self.status,
            SqlModalStatus::ConfirmingHigh { .. }
                | SqlModalStatus::ConfirmingAnalyzeHigh { .. }
                | SqlModalStatus::ConfirmingRisk { .. }
                | SqlModalStatus::ConfirmingAnalyzeRisk { .. }
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

    pub fn accept_selected_completion(&mut self, visible_rows: usize) {
        let Some((trigger_pos, replacement)) = self.selected_completion_replacement() else {
            return;
        };
        if self.editor.cursor() < trigger_pos {
            self.dismiss_completion();
            return;
        }

        let start_byte = self.editor.char_to_byte_index(trigger_pos);
        let end_byte = self.editor.char_to_byte_index(self.editor.cursor());
        let mut content = self.editor.content().to_string();
        content.drain(start_byte..end_byte);
        content.insert_str(start_byte, &replacement);
        let new_cursor = trigger_pos + replacement.chars().count();
        self.editor.set_content_with_cursor(content, new_cursor);
        self.editor.update_scroll(visible_rows);
        self.dismiss_completion();
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
            let _ = ctx.begin_prefetch();
            ctx.queue_table_prefetch("public.users".to_string());
            ctx.start_table_prefetch("public.posts".to_string());
            ctx.fail_table_prefetch(
                "public.failed".to_string(),
                FailedPrefetchEntry {
                    failed_at: Instant::now(),
                    error: "error".to_string(),
                    retry_count: 0,
                },
            );

            ctx.reset_prefetch();

            assert!(!ctx.is_prefetch_started());
            assert!(ctx.prefetch_queue().is_empty());
            assert!(ctx.prefetching_tables().is_empty());
            assert!(ctx.failed_prefetch_tables().is_empty());
        }

        #[test]
        fn queueing_skips_queued_and_in_flight_tables() {
            let mut ctx = SqlModalContext::default();
            ctx.queue_table_prefetch("public.users".to_string());
            ctx.queue_table_prefetch("public.users".to_string());
            ctx.start_table_prefetch("public.orders".to_string());
            ctx.queue_table_prefetch("public.orders".to_string());

            assert_eq!(ctx.prefetch_queue().len(), 1);
            assert!(ctx.is_prefetch_queued("public.users"));
            assert!(ctx.is_table_prefetching("public.orders"));
            assert_eq!(ctx.prefetch_in_flight_count(), 1);
        }

        #[test]
        fn retry_table_prefetch_preserves_failure_and_requeues_table() {
            let mut ctx = SqlModalContext::default();
            let failed_at = Instant::now();

            ctx.start_table_prefetch("public.users".to_string());
            ctx.retry_table_prefetch(
                "public.users".to_string(),
                FailedPrefetchEntry {
                    failed_at,
                    error: "timeout".to_string(),
                    retry_count: 1,
                },
            );

            assert!(!ctx.is_table_prefetching("public.users"));
            assert!(ctx.is_prefetch_queued("public.users"));
            assert_eq!(ctx.failed_prefetch("public.users").unwrap().retry_count, 1);
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
                target_name: "users".to_string(),
            };

            assert!(matches!(
                status,
                SqlModalStatus::ConfirmingHigh { ref target_name, .. } if target_name == "users"
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
                "users".to_string(),
            );
            ctx.cancel_confirmation();
            assert_eq!(ctx.status, SqlModalStatus::Normal);
        }

        #[test]
        fn begin_confirming_risk_sets_status_and_dismisses_completion() {
            let mut ctx = SqlModalContext::default();
            ctx.completion.visible = true;

            ctx.begin_confirming_risk(AcknowledgeReason::UnknownRisk, "DO".to_string());

            assert!(matches!(
                ctx.status,
                SqlModalStatus::ConfirmingRisk {
                    reason: AcknowledgeReason::UnknownRisk,
                    ref label,
                } if label == "DO"
            ));
            assert!(!ctx.completion.visible);
        }

        #[test]
        fn begin_confirming_analyze_risk_switches_to_plan_tab() {
            let mut ctx = SqlModalContext::default();

            ctx.begin_confirming_analyze_risk(
                "MERGE INTO t USING s ON t.id = s.id".to_string(),
                AcknowledgeReason::UnknownRisk,
            );

            assert!(matches!(
                ctx.status,
                SqlModalStatus::ConfirmingAnalyzeRisk { .. }
            ));
            assert_eq!(ctx.active_tab, SqlModalTab::Plan);
        }

        #[test]
        fn cancel_resets_risk_confirmation_to_normal() {
            let mut ctx = SqlModalContext::default();
            ctx.begin_confirming_risk(AcknowledgeReason::TargetNameUnavailable, "DROP".to_string());

            ctx.cancel_confirmation();

            assert_eq!(ctx.status, SqlModalStatus::Normal);
        }

        #[test]
        fn cancel_resets_analyze_risk_confirmation_to_normal() {
            let mut ctx = SqlModalContext::default();
            ctx.begin_confirming_analyze_risk(
                "GRANT SELECT ON users TO role1".to_string(),
                AcknowledgeReason::UnknownRisk,
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
