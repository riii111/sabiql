use std::sync::Arc;
use std::time::Instant;

use crate::domain::{QueryResult, QuerySource};
use crate::model::browse::result_history::ResultHistory;
use crate::model::shared::async_run::AsyncRun;

pub const PREVIEW_PAGE_SIZE: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibleResultKind {
    LivePreview,
    LiveAdhoc,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryStatus {
    #[default]
    Idle,
    Running,
}

#[derive(Debug, Clone, Default)]
pub struct PaginationState {
    pub current_page: usize,
    pub total_rows_estimate: Option<i64>,
    pub reached_end: bool,
    pub schema: String,
    pub table: String,
}

impl PaginationState {
    pub fn offset(&self) -> usize {
        self.current_page * PREVIEW_PAGE_SIZE
    }

    pub fn total_pages_estimate(&self) -> Option<usize> {
        self.total_rows_estimate.map(|total| {
            let total = total.max(0) as usize;
            total.div_ceil(PREVIEW_PAGE_SIZE).max(1)
        })
    }

    pub fn can_next(&self) -> bool {
        !self.reached_end
    }

    pub fn can_prev(&self) -> bool {
        self.current_page > 0
    }

    pub fn reset(&mut self) {
        self.current_page = 0;
        self.total_rows_estimate = None;
        self.reached_end = false;
        self.schema.clear();
        self.table.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PostDeleteRowSelection {
    #[default]
    Keep,
    Clear,
    Select(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteRefreshTarget {
    pub target_page: usize,
    pub target_row: Option<usize>,
    pub expected_delete_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct QueryExecution {
    status: QueryStatus,
    start_time: Option<Instant>,
    current_result: Option<Arc<QueryResult>>,
    result_history: ResultHistory,
    result_generation: u64,
    result_highlight_until: Option<Instant>,
    pub pagination: PaginationState,
    pending_delete_refresh_target: Option<DeleteRefreshTarget>,
    post_delete_row_selection: PostDeleteRowSelection,
    run: AsyncRun,
}

impl QueryExecution {
    // ── Status / timing ────────────────────────────────────────────

    #[must_use]
    pub fn begin_running(&mut self, now: Instant) -> u64 {
        self.status = QueryStatus::Running;
        self.start_time = Some(now);
        self.run.begin()
    }

    pub fn mark_idle(&mut self) {
        self.status = QueryStatus::Idle;
        self.start_time = None;
        self.run.clear_active();
    }

    pub fn is_current_run(&self, run_id: u64) -> bool {
        self.run.is_current(run_id)
    }

    pub fn status(&self) -> QueryStatus {
        self.status
    }

    pub fn start_time(&self) -> Option<Instant> {
        self.start_time
    }

    pub fn is_running(&self) -> bool {
        self.status == QueryStatus::Running
    }

    // ── Current result ──────────────────────────────────────────────

    pub fn set_current_result(&mut self, result: Arc<QueryResult>) {
        self.current_result = Some(result);
        self.result_generation += 1;
    }

    pub fn clear_current_result(&mut self) {
        self.current_result = None;
        self.result_generation += 1;
    }

    pub fn push_history(&mut self, result: Arc<QueryResult>) {
        self.result_history.push(result);
        self.result_generation += 1;
    }

    pub fn result_generation(&self) -> u64 {
        self.result_generation
    }

    pub fn result_history(&self) -> &ResultHistory {
        &self.result_history
    }

    pub fn restore_history(&mut self, history: ResultHistory) {
        self.result_history = history;
        self.result_generation += 1;
    }

    pub fn current_result(&self) -> Option<&Arc<QueryResult>> {
        self.current_result.as_ref()
    }

    // ── Result highlight ────────────────────────────────────────────

    pub fn set_result_highlight(&mut self, until: Instant) {
        self.result_highlight_until = Some(until);
    }

    pub fn clear_expired_highlight(&mut self, now: Instant) {
        if let Some(until) = self.result_highlight_until
            && now >= until
        {
            self.result_highlight_until = None;
        }
    }

    pub fn result_highlight_until(&self) -> Option<Instant> {
        self.result_highlight_until
    }

    // ── Delete lifecycle ─────────────────────────────────────────────

    pub fn set_delete_refresh_target(&mut self, page: usize, row: Option<usize>, count: usize) {
        self.pending_delete_refresh_target = Some(DeleteRefreshTarget {
            target_page: page,
            target_row: row,
            expected_delete_count: count,
        });
    }

    pub fn take_delete_refresh_target(&mut self) -> Option<DeleteRefreshTarget> {
        self.pending_delete_refresh_target.take()
    }

    pub fn clear_delete_refresh_target(&mut self) {
        self.pending_delete_refresh_target = None;
    }

    pub fn pending_delete_refresh_target(&self) -> Option<DeleteRefreshTarget> {
        self.pending_delete_refresh_target
    }

    pub fn set_post_delete_selection(&mut self, sel: PostDeleteRowSelection) {
        self.post_delete_row_selection = sel;
    }

    pub fn post_delete_row_selection(&self) -> PostDeleteRowSelection {
        self.post_delete_row_selection
    }

    // ── Visible result ─────────────────────────────────────────────

    pub fn visible_result_kind(&self) -> VisibleResultKind {
        match &self.current_result {
            Some(r) => match r.source {
                QuerySource::Preview => VisibleResultKind::LivePreview,
                QuerySource::Adhoc => VisibleResultKind::LiveAdhoc,
            },
            None => VisibleResultKind::Empty,
        }
    }

    pub fn visible_result(&self) -> Option<&QueryResult> {
        self.current_result.as_deref()
    }

    pub fn can_edit_visible_result(&self) -> bool {
        self.visible_result_kind() == VisibleResultKind::LivePreview
            && self
                .visible_result()
                .is_some_and(|result| !result.is_error())
    }

    pub fn can_paginate_visible_result(&self) -> bool {
        self.visible_result_kind() == VisibleResultKind::LivePreview
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::QuerySource;

    fn make_result(source: QuerySource) -> Arc<QueryResult> {
        Arc::new(QueryResult::success(
            "SELECT 1".to_string(),
            vec!["col".to_string()],
            vec![vec!["val".to_string()]],
            10,
            source,
        ))
    }

    mod visible_result_kind_tests {
        use super::*;

        #[test]
        fn empty_when_no_result_and_no_history() {
            let qe = QueryExecution::default();

            assert_eq!(qe.visible_result_kind(), VisibleResultKind::Empty);
        }

        #[test]
        fn live_preview_when_current_result_is_preview() {
            let qe = QueryExecution {
                current_result: Some(make_result(QuerySource::Preview)),
                ..Default::default()
            };

            assert_eq!(qe.visible_result_kind(), VisibleResultKind::LivePreview);
        }

        #[test]
        fn live_adhoc_when_current_result_is_adhoc() {
            let qe = QueryExecution {
                current_result: Some(make_result(QuerySource::Adhoc)),
                ..Default::default()
            };

            assert_eq!(qe.visible_result_kind(), VisibleResultKind::LiveAdhoc);
        }
    }

    mod visible_result_tests {
        use super::*;

        #[test]
        fn current_result_when_present() {
            let qe = QueryExecution {
                current_result: Some(make_result(QuerySource::Preview)),
                ..Default::default()
            };

            assert!(qe.visible_result().is_some());
            assert_eq!(qe.visible_result().unwrap().source, QuerySource::Preview);
        }

        #[test]
        fn history_does_not_replace_current_result() {
            let mut qe = QueryExecution::default();
            qe.result_history.push(make_result(QuerySource::Adhoc));
            qe.current_result = Some(make_result(QuerySource::Preview));

            assert!(qe.visible_result().is_some());
            assert_eq!(qe.visible_result().unwrap().source, QuerySource::Preview);
        }

        #[test]
        fn empty_query_execution_returns_none() {
            let qe = QueryExecution::default();

            assert!(qe.visible_result().is_none());
        }

        #[test]
        fn history_without_live_result_returns_none() {
            let mut qe = QueryExecution::default();
            qe.result_history.push(make_result(QuerySource::Adhoc));

            assert!(qe.visible_result().is_none());
        }
    }

    mod capability_tests {
        use super::*;

        #[test]
        fn can_edit_only_live_preview() {
            let preview = QueryExecution {
                current_result: Some(make_result(QuerySource::Preview)),
                ..Default::default()
            };
            let preview_error = QueryExecution {
                current_result: Some(Arc::new(QueryResult::error(
                    "SELECT 1".to_string(),
                    "boom".to_string(),
                    10,
                    QuerySource::Preview,
                ))),
                ..Default::default()
            };
            let adhoc = QueryExecution {
                current_result: Some(make_result(QuerySource::Adhoc)),
                ..Default::default()
            };
            let empty = QueryExecution::default();

            assert!(preview.can_edit_visible_result());
            assert!(!preview_error.can_edit_visible_result());
            assert!(!adhoc.can_edit_visible_result());
            assert!(!empty.can_edit_visible_result());
        }

        #[test]
        fn can_paginate_only_live_preview() {
            let preview = QueryExecution {
                current_result: Some(make_result(QuerySource::Preview)),
                ..Default::default()
            };
            let adhoc = QueryExecution {
                current_result: Some(make_result(QuerySource::Adhoc)),
                ..Default::default()
            };

            assert!(preview.can_paginate_visible_result());
            assert!(!adhoc.can_paginate_visible_result());
        }
    }

    #[test]
    fn default_creates_idle_state() {
        let execution = QueryExecution::default();

        assert_eq!(execution.status(), QueryStatus::Idle);
        assert!(execution.start_time().is_none());
        assert!(execution.current_result().is_none());
        assert_eq!(execution.result_generation(), 0);
    }

    mod result_generation_tests {
        use super::*;

        #[test]
        fn increments_on_set_current_result() {
            let mut qe = QueryExecution::default();
            assert_eq!(qe.result_generation(), 0);

            qe.set_current_result(make_result(QuerySource::Preview));
            assert_eq!(qe.result_generation(), 1);

            qe.set_current_result(make_result(QuerySource::Adhoc));
            assert_eq!(qe.result_generation(), 2);
        }

        #[test]
        fn increments_on_clear_current_result() {
            let mut qe = QueryExecution::default();
            qe.set_current_result(make_result(QuerySource::Preview));

            qe.clear_current_result();
            assert_eq!(qe.result_generation(), 2);
        }

        #[test]
        fn increments_on_push_history() {
            let mut qe = QueryExecution::default();

            qe.push_history(make_result(QuerySource::Adhoc));
            assert_eq!(qe.result_generation(), 1);
            assert_eq!(qe.result_history.len(), 1);
        }

        #[test]
        fn does_not_increment_on_cursor_like_operations() {
            let mut qe = QueryExecution::default();
            qe.set_current_result(make_result(QuerySource::Preview));
            let before = qe.result_generation();

            // These should not change generation
            let _ = qe.visible_result();
            let _ = qe.visible_result_kind();

            assert_eq!(qe.result_generation(), before);
        }
    }

    #[test]
    fn query_status_default_is_idle() {
        assert_eq!(QueryStatus::default(), QueryStatus::Idle);
    }

    mod pagination {
        use super::*;

        #[test]
        fn offset_returns_correct_value() {
            let p = PaginationState {
                current_page: 3,
                ..Default::default()
            };

            assert_eq!(p.offset(), 3 * PREVIEW_PAGE_SIZE);
        }

        #[test]
        fn total_pages_estimate_rounds_up() {
            let p = PaginationState {
                total_rows_estimate: Some(1001),
                ..Default::default()
            };

            assert_eq!(p.total_pages_estimate(), Some(3));
        }

        #[test]
        fn total_pages_estimate_exact_division() {
            let p = PaginationState {
                total_rows_estimate: Some(1000),
                ..Default::default()
            };

            assert_eq!(p.total_pages_estimate(), Some(2));
        }

        #[test]
        fn total_pages_estimate_none_when_unknown() {
            let p = PaginationState::default();

            assert_eq!(p.total_pages_estimate(), None);
        }

        #[test]
        fn total_pages_estimate_clamps_zero_to_one() {
            let p = PaginationState {
                total_rows_estimate: Some(0),
                ..Default::default()
            };

            assert_eq!(p.total_pages_estimate(), Some(1));
        }

        #[test]
        fn total_pages_estimate_clamps_negative_to_one() {
            let p = PaginationState {
                total_rows_estimate: Some(-1),
                ..Default::default()
            };

            assert_eq!(p.total_pages_estimate(), Some(1));
        }

        #[test]
        fn can_next_false_when_reached_end() {
            let p = PaginationState {
                reached_end: true,
                ..Default::default()
            };

            assert!(!p.can_next());
        }

        #[test]
        fn can_next_true_when_estimate_unknown() {
            let p = PaginationState::default();

            assert!(p.can_next());
        }

        #[test]
        fn can_prev_false_on_first_page() {
            let p = PaginationState::default();

            assert!(!p.can_prev());
        }

        #[test]
        fn can_prev_true_on_later_page() {
            let p = PaginationState {
                current_page: 2,
                ..Default::default()
            };

            assert!(p.can_prev());
        }

        #[test]
        fn reset_clears_state() {
            let mut p = PaginationState {
                current_page: 5,
                total_rows_estimate: Some(10000),
                reached_end: true,
                schema: "public".to_string(),
                table: "users".to_string(),
            };

            p.reset();

            assert_eq!(p.current_page, 0);
            assert_eq!(p.total_rows_estimate, None);
            assert!(!p.reached_end);
            assert!(p.schema.is_empty());
            assert!(p.table.is_empty());
        }
    }
}
