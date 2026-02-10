use std::sync::Arc;
use std::time::Instant;

use crate::app::result_history::ResultHistory;
use crate::domain::QueryResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryStatus {
    #[default]
    Idle,
    Running,
}

#[derive(Debug, Clone, Default)]
pub struct QueryExecution {
    pub status: QueryStatus,
    pub start_time: Option<Instant>,
    pub current_result: Option<Arc<QueryResult>>,
    pub result_history: ResultHistory,
    pub history_index: Option<usize>,
    pub result_highlight_until: Option<Instant>,
    pub perf_started_at: Option<Instant>,
    pub perf_completed_at: Option<Instant>,
    pub perf_render_pending: bool,
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
}
