use std::sync::Arc;
use std::time::Instant;

use crate::app::result_history::ResultHistory;
use crate::domain::QueryResult;

pub const PREVIEW_PAGE_SIZE: usize = 500;

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

#[derive(Debug, Clone, Default)]
pub struct QueryExecution {
    pub status: QueryStatus,
    pub start_time: Option<Instant>,
    pub current_result: Option<Arc<QueryResult>>,
    pub result_history: ResultHistory,
    pub history_index: Option<usize>,
    pub result_highlight_until: Option<Instant>,
    pub pagination: PaginationState,
    pub pending_row_selection_after_refresh: Option<usize>,
    pub clear_row_selection_after_refresh: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_creates_idle_state() {
        let execution = QueryExecution::default();

        assert_eq!(execution.status, QueryStatus::Idle);
        assert!(execution.start_time.is_none());
        assert!(execution.current_result.is_none());
        assert!(execution.history_index.is_none());
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
