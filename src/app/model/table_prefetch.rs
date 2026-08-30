use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::model::shared::async_run::AsyncRun;

pub(crate) const MAX_PREFETCH_RETRIES: u32 = 3;

#[derive(Debug, Clone)]
pub struct FailedPrefetchEntry {
    pub failed_at: Instant,
    pub error: String,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct TablePrefetchState {
    prefetch_queue: VecDeque<String>,
    prefetching_tables: HashSet<String>,
    failed_prefetch_tables: HashMap<String, FailedPrefetchEntry>,
    prefetch_tracks_er: bool,
    prefetch_run: AsyncRun,
}

impl TablePrefetchState {
    pub fn reset_prefetch(&mut self) {
        self.prefetch_queue.clear();
        self.prefetching_tables.clear();
        self.failed_prefetch_tables.clear();
        self.prefetch_tracks_er = false;
        self.prefetch_run.clear_active();
    }

    #[must_use]
    pub fn begin_er_prefetch(&mut self) -> u64 {
        if self.prefetch_run.active_id().is_some() && !self.prefetch_tracks_er {
            // Responses from the replaced completion run are stale and will be discarded.
            self.prefetching_tables.clear();
        }
        self.prefetch_tracks_er = true;
        self.prefetch_queue.clear();
        self.failed_prefetch_tables.clear();
        self.prefetch_run.begin()
    }

    #[must_use]
    pub fn begin_completion_prefetch(&mut self) -> u64 {
        self.prefetch_tracks_er = false;
        self.prefetch_queue.clear();
        self.failed_prefetch_tables.clear();
        self.prefetch_run.begin()
    }

    pub fn prefetch_tracks_er(&self) -> bool {
        self.prefetch_tracks_er
    }

    pub fn invalidate_prefetch(&mut self) {
        self.prefetch_tracks_er = false;
        self.prefetching_tables.clear();
        self.prefetch_run.clear_active();
    }

    pub fn has_pending_prefetch(&self) -> bool {
        !self.prefetch_queue.is_empty()
    }

    pub fn pending_prefetch_count(&self) -> usize {
        self.prefetch_queue.len()
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

    pub fn failed_table_errors(&self) -> Vec<(String, String)> {
        self.failed_prefetch_tables
            .iter()
            .filter(|(table, entry)| self.is_permanent_failure(table, entry))
            .map(|(table, entry)| (table.clone(), entry.error.clone()))
            .collect()
    }

    pub fn has_failures(&self) -> bool {
        self.failed_prefetch_count() > 0
    }

    pub fn failed_prefetch_count(&self) -> usize {
        self.failed_prefetch_tables
            .iter()
            .filter(|(table, entry)| self.is_permanent_failure(table, entry))
            .count()
    }

    pub fn queue_table_prefetch(&mut self, table: String) {
        if self.prefetching_tables.contains(&table) || self.is_prefetch_queued(&table) {
            return;
        }
        self.prefetch_queue.push_back(table);
    }

    pub fn queue_pending_table(&mut self, table: String) -> bool {
        if self.prefetching_tables.contains(&table) || self.is_prefetch_queued(&table) {
            return false;
        }
        self.failed_prefetch_tables.remove(&table);
        self.prefetch_queue.push_back(table);
        true
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

    pub fn retry_table_prefetch(&mut self, table: String, entry: FailedPrefetchEntry) -> bool {
        let had_other_pending_before_requeue = self.has_pending_prefetch();
        self.fail_table_prefetch(table.clone(), entry);
        self.queue_table_prefetch(table);
        had_other_pending_before_requeue
    }

    pub fn active_prefetch_run_id(&self) -> Option<u64> {
        self.prefetch_run.active_id()
    }

    pub fn is_current_prefetch_run(&self, run_id: u64) -> bool {
        self.prefetch_run.is_current(run_id)
    }

    pub fn is_complete(&self) -> bool {
        self.prefetch_queue.is_empty() && self.prefetching_tables.is_empty()
    }

    fn is_permanent_failure(&self, table: &str, entry: &FailedPrefetchEntry) -> bool {
        entry.retry_count >= MAX_PREFETCH_RETRIES
            && !self.is_prefetch_queued(table)
            && !self.prefetching_tables.contains(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_clears_queue_in_flight_failures_and_run() {
        let mut state = TablePrefetchState::default();
        let _ = state.begin_er_prefetch();
        state.queue_table_prefetch("public.users".to_string());
        state.start_table_prefetch("public.posts".to_string());
        state.fail_table_prefetch(
            "public.failed".to_string(),
            FailedPrefetchEntry {
                failed_at: Instant::now(),
                error: "error".to_string(),
                retry_count: 0,
            },
        );

        state.reset_prefetch();

        assert!(state.active_prefetch_run_id().is_none());
        assert!(!state.has_pending_prefetch());
        assert_eq!(state.prefetch_in_flight_count(), 0);
        assert!(state.failed_prefetch("public.failed").is_none());
    }

    #[test]
    fn queueing_skips_queued_and_in_flight_tables() {
        let mut state = TablePrefetchState::default();
        state.queue_table_prefetch("public.users".to_string());
        state.queue_table_prefetch("public.users".to_string());
        state.start_table_prefetch("public.orders".to_string());
        state.queue_table_prefetch("public.orders".to_string());

        assert!(state.has_pending_prefetch());
        assert!(state.is_prefetch_queued("public.users"));
        assert!(state.is_table_prefetching("public.orders"));
        assert_eq!(state.prefetch_in_flight_count(), 1);
    }

    #[test]
    fn retry_preserves_failure_and_requeues_table() {
        let mut state = TablePrefetchState::default();
        let failed_at = Instant::now();

        state.start_table_prefetch("public.users".to_string());
        state.retry_table_prefetch(
            "public.users".to_string(),
            FailedPrefetchEntry {
                failed_at,
                error: "timeout".to_string(),
                retry_count: 1,
            },
        );

        assert!(!state.is_table_prefetching("public.users"));
        assert!(state.is_prefetch_queued("public.users"));
        assert_eq!(
            state.failed_prefetch("public.users").unwrap().retry_count,
            1
        );
    }

    #[test]
    fn completion_ignores_permanent_failures_after_queue_drains() {
        let mut state = TablePrefetchState::default();
        state.fail_table_prefetch(
            "public.users".to_string(),
            FailedPrefetchEntry {
                failed_at: Instant::now(),
                error: "timeout".to_string(),
                retry_count: 3,
            },
        );

        assert!(state.is_complete());
        assert!(state.has_failures());
    }

    #[test]
    fn retry_failure_is_not_counted_until_it_becomes_permanent() {
        let mut state = TablePrefetchState::default();
        state.start_table_prefetch("public.users".to_string());
        state.retry_table_prefetch(
            "public.users".to_string(),
            FailedPrefetchEntry {
                failed_at: Instant::now(),
                error: "timeout".to_string(),
                retry_count: 1,
            },
        );

        assert_eq!(state.failed_prefetch_count(), 0);
        assert!(!state.has_failures());

        let _ = state.take_next_prefetch();
        state.fail_table_prefetch(
            "public.users".to_string(),
            FailedPrefetchEntry {
                failed_at: Instant::now(),
                error: "timeout".to_string(),
                retry_count: MAX_PREFETCH_RETRIES,
            },
        );

        assert_eq!(state.failed_prefetch_count(), 1);
        assert!(state.has_failures());
    }
}
