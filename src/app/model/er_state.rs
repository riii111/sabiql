use std::collections::{HashMap, HashSet};

use crate::model::shared::async_run::AsyncRun;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErStatus {
    #[default]
    Idle,
    Waiting,
    Rendering,
}

#[derive(Debug, Clone, Default)]
pub struct ErPreparationState {
    pending_tables: HashSet<String>,
    fetching_tables: HashSet<String>,
    failed_tables: HashMap<String, String>,
    status: ErStatus,
    total_tables: usize,
    target_tables: Vec<String>,
    seed_tables: Vec<String>,
    fk_expanded: bool,
    last_signatures: HashMap<String, String>,
    run: AsyncRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErPreparationProgress {
    pub cached: usize,
    pub total: usize,
    pub failed: usize,
    pub remaining: usize,
}

impl ErPreparationState {
    pub fn status(&self) -> ErStatus {
        self.status
    }

    pub fn is_waiting(&self) -> bool {
        self.status == ErStatus::Waiting
    }

    pub fn progress(&self) -> ErPreparationProgress {
        let failed = self.failed_tables.len();
        let remaining = self.pending_tables.len() + self.fetching_tables.len();
        let cached = self.total_tables.saturating_sub(remaining + failed);
        ErPreparationProgress {
            cached,
            total: self.total_tables,
            failed,
            remaining,
        }
    }

    pub fn failed_table_errors(&self) -> Vec<(String, String)> {
        self.failed_tables
            .iter()
            .map(|(table, error)| (table.clone(), error.clone()))
            .collect()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn pending_tables(&self) -> &HashSet<String> {
        &self.pending_tables
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn fetching_tables(&self) -> &HashSet<String> {
        &self.fetching_tables
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn failed_tables(&self) -> &HashMap<String, String> {
        &self.failed_tables
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn total_tables(&self) -> usize {
        self.total_tables
    }

    pub fn target_tables(&self) -> &[String] {
        &self.target_tables
    }

    pub fn seed_tables(&self) -> &[String] {
        &self.seed_tables
    }

    pub fn fk_expanded(&self) -> bool {
        self.fk_expanded
    }

    pub fn last_signatures(&self) -> &HashMap<String, String> {
        &self.last_signatures
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.status, ErStatus::Rendering | ErStatus::Waiting)
    }

    pub fn is_complete(&self) -> bool {
        self.pending_tables.is_empty() && self.fetching_tables.is_empty()
    }

    pub fn has_failures(&self) -> bool {
        !self.failed_tables.is_empty()
    }

    pub fn on_table_cached(&mut self, qualified_name: &str) {
        self.fetching_tables.remove(qualified_name);
        self.pending_tables.remove(qualified_name);
    }

    pub fn on_table_failed(&mut self, qualified_name: &str, error: String) {
        self.fetching_tables.remove(qualified_name);
        self.pending_tables.remove(qualified_name);
        self.failed_tables.insert(qualified_name.to_string(), error);
    }

    pub fn start_fetching(&mut self, qualified_name: &str) {
        self.pending_tables.remove(qualified_name);
        self.fetching_tables.insert(qualified_name.to_string());
    }

    pub fn requeue_for_retry(&mut self, qualified_name: &str) {
        self.fetching_tables.remove(qualified_name);
        self.pending_tables.insert(qualified_name.to_string());
    }

    pub fn begin_all_prefetch<I>(&mut self, tables: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.pending_tables.clear();
        self.fetching_tables.clear();
        self.failed_tables.clear();
        self.total_tables = 0;
        self.fk_expanded = true;

        for table in tables {
            self.pending_tables.insert(table);
            self.total_tables += 1;
        }
    }

    pub fn begin_scoped_prefetch(&mut self, tables: impl AsRef<[String]>) {
        let tables = tables.as_ref();
        self.pending_tables.clear();
        self.fetching_tables.clear();
        self.failed_tables.clear();
        self.fk_expanded = false;
        self.seed_tables = tables.to_vec();
        self.total_tables = tables.len();
        self.pending_tables.extend(tables.iter().cloned());
    }

    pub fn retry_failed(&mut self) {
        for (table, _) in self.failed_tables.drain() {
            self.pending_tables.insert(table);
        }
    }

    pub fn mark_idle(&mut self) {
        self.status = ErStatus::Idle;
    }

    pub fn mark_waiting(&mut self) {
        self.status = ErStatus::Waiting;
    }

    pub fn is_current_run(&self, run_id: u64) -> bool {
        self.run.is_current(run_id)
    }

    pub fn run_id(&self) -> u64 {
        self.run.last_id()
    }

    pub fn can_generate_from_cache(&self) -> bool {
        matches!(self.status, ErStatus::Idle | ErStatus::Waiting)
    }

    pub fn reset(&mut self) {
        self.pending_tables.clear();
        self.fetching_tables.clear();
        self.failed_tables.clear();
        self.status = ErStatus::Idle;
        self.total_tables = 0;
        self.target_tables.clear();
        self.seed_tables.clear();
        self.fk_expanded = false;
        self.last_signatures.clear();
        self.run.clear_active();
    }

    pub fn mark_rendering(&mut self) {
        self.status = ErStatus::Rendering;
    }

    #[must_use]
    pub fn start_waiting_run(&mut self) -> u64 {
        let run_id = self.run.begin();
        self.status = ErStatus::Waiting;
        run_id
    }

    pub fn begin_full_prefetch(&mut self, total: usize) {
        self.clear_table_tracking();
        self.total_tables = total;
        self.seed_tables.clear();
        self.fk_expanded = true;
    }

    pub fn set_targets(&mut self, tables: Vec<String>) {
        self.target_tables = tables;
    }

    pub fn mark_fk_expanded(&mut self) {
        self.fk_expanded = true;
    }

    pub fn mark_fk_unexpanded(&mut self) {
        self.fk_expanded = false;
    }

    pub fn apply_refresh_metadata(
        &mut self,
        signatures: HashMap<String, String>,
        total_tables: usize,
    ) {
        self.last_signatures = signatures;
        self.total_tables = total_tables;
    }

    pub fn invalidate_refresh_signatures(&mut self, total_tables: usize) {
        self.last_signatures.clear();
        self.total_tables = total_tables;
    }

    pub fn scoped_fallback_tables(&self, total_table_count: usize) -> Option<Vec<String>> {
        if !self.target_tables.is_empty() && self.target_tables.len() < total_table_count {
            Some(self.target_tables.clone())
        } else {
            None
        }
    }

    fn clear_table_tracking(&mut self) {
        self.pending_tables.clear();
        self.fetching_tables.clear();
        self.failed_tables.clear();
    }

    // Re-queueing a table starts a fresh attempt and clears any prior failure
    // record for that table.
    pub fn queue_pending_table(&mut self, table: String) -> bool {
        self.fetching_tables.remove(&table);
        self.failed_tables.remove(&table);
        self.pending_tables.insert(table)
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn insert_fetching_table(&mut self, table: String) {
        self.pending_tables.remove(&table);
        self.fetching_tables.insert(table);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_active_run_id(state: &mut ErPreparationState, run_id: u64) {
        for _ in 0..run_id {
            let _ = state.start_waiting_run();
        }
        state.mark_idle();
    }

    mod is_complete {
        use super::*;

        #[test]
        fn empty_state_returns_true() {
            let state = ErPreparationState::default();

            assert!(state.is_complete());
        }

        #[test]
        fn pending_tables_returns_false() {
            let mut state = ErPreparationState::default();
            state.pending_tables.insert("public.users".to_string());

            assert!(!state.is_complete());
        }

        #[test]
        fn fetching_tables_returns_false() {
            let mut state = ErPreparationState::default();
            state.fetching_tables.insert("public.users".to_string());

            assert!(!state.is_complete());
        }
    }

    mod on_table_cached {
        use super::*;

        #[test]
        fn removes_from_fetching() {
            let mut state = ErPreparationState::default();
            state.fetching_tables.insert("public.users".to_string());

            state.on_table_cached("public.users");

            assert!(!state.fetching_tables.contains("public.users"));
        }

        #[test]
        fn removes_from_pending() {
            let mut state = ErPreparationState::default();
            state.pending_tables.insert("public.users".to_string());

            state.on_table_cached("public.users");

            assert!(!state.pending_tables.contains("public.users"));
        }
    }

    mod on_table_failed {
        use super::*;

        #[test]
        fn moves_from_fetching_to_failed() {
            let mut state = ErPreparationState::default();
            state.fetching_tables.insert("public.users".to_string());

            state.on_table_failed("public.users", "timeout".to_string());

            assert!(!state.fetching_tables.contains("public.users"));
            assert!(state.failed_tables.contains_key("public.users"));
        }

        #[test]
        fn removes_from_pending() {
            let mut state = ErPreparationState::default();
            state.pending_tables.insert("public.users".to_string());

            state.on_table_failed("public.users", "timeout".to_string());

            assert!(!state.pending_tables.contains("public.users"));
        }
    }

    mod requeue_for_retry {
        use super::*;

        #[test]
        fn moves_from_fetching_to_pending_without_failed_state() {
            let mut state = ErPreparationState::default();
            state.fetching_tables.insert("public.users".to_string());

            state.requeue_for_retry("public.users");

            assert!(!state.fetching_tables.contains("public.users"));
            assert!(state.pending_tables.contains("public.users"));
            assert!(!state.failed_tables.contains_key("public.users"));
        }
    }

    mod start_fetching {
        use super::*;

        #[test]
        fn moves_from_pending_to_fetching() {
            let mut state = ErPreparationState::default();
            state.pending_tables.insert("public.users".to_string());

            state.start_fetching("public.users");

            assert!(!state.pending_tables.contains("public.users"));
            assert!(state.fetching_tables.contains("public.users"));
        }
    }

    mod begin_prefetch {
        use super::*;

        #[test]
        fn all_prefetch_replaces_progress_and_marks_fk_expanded() {
            let mut state = ErPreparationState {
                pending_tables: HashSet::from(["stale.pending".to_string()]),
                fetching_tables: HashSet::from(["stale.fetching".to_string()]),
                failed_tables: HashMap::from([("stale.failed".to_string(), "error".to_string())]),
                fk_expanded: false,
                ..Default::default()
            };

            state.begin_all_prefetch(vec![
                "public.users".to_string(),
                "public.orders".to_string(),
            ]);

            assert_eq!(state.total_tables, 2);
            assert!(state.fk_expanded);
            assert!(state.fetching_tables.is_empty());
            assert!(state.failed_tables.is_empty());
            assert!(state.pending_tables.contains("public.users"));
            assert!(state.pending_tables.contains("public.orders"));
        }

        #[test]
        fn scoped_prefetch_records_seed_tables_and_pending_set() {
            let mut state = ErPreparationState {
                fk_expanded: true,
                ..Default::default()
            };
            let tables = vec!["public.users".to_string(), "public.orders".to_string()];

            state.begin_scoped_prefetch(&tables);

            assert_eq!(state.total_tables, 2);
            assert!(!state.fk_expanded);
            assert_eq!(state.seed_tables, tables);
            assert!(state.pending_tables.contains("public.users"));
            assert!(state.pending_tables.contains("public.orders"));
        }
    }

    mod retry_failed {
        use super::*;

        #[test]
        fn moves_failed_to_pending() {
            let mut state = ErPreparationState::default();
            state
                .failed_tables
                .insert("public.users".to_string(), "error".to_string());

            state.retry_failed();

            assert!(state.failed_tables.is_empty());
            assert!(state.pending_tables.contains("public.users"));
        }
    }

    mod reset {
        use super::*;

        #[test]
        fn clears_all_state() {
            let mut state = ErPreparationState {
                pending_tables: HashSet::from(["a".to_string()]),
                fetching_tables: HashSet::from(["b".to_string()]),
                failed_tables: HashMap::from([("c".to_string(), "err".to_string())]),
                status: ErStatus::Waiting,
                total_tables: 3,
                seed_tables: vec!["a".to_string()],
                fk_expanded: true,
                last_signatures: HashMap::from([("a".to_string(), "sig".to_string())]),
                ..Default::default()
            };
            set_active_run_id(&mut state, 5);

            state.reset();

            assert!(state.pending_tables.is_empty());
            assert!(state.fetching_tables.is_empty());
            assert!(state.failed_tables.is_empty());
            assert_eq!(state.status, ErStatus::Idle);
            assert_eq!(state.total_tables, 0);
            assert!(state.seed_tables.is_empty());
            assert!(!state.fk_expanded);
            assert!(state.last_signatures.is_empty());
            assert_eq!(state.run_id(), 5);
        }
    }

    mod waiting_resolution {
        use super::*;

        #[test]
        fn skip_only_completion_becomes_ready() {
            let mut state = ErPreparationState {
                pending_tables: HashSet::from(["public.users".to_string()]),
                fetching_tables: HashSet::new(),
                failed_tables: HashMap::new(),
                status: ErStatus::Waiting,
                total_tables: 1,
                target_tables: vec![],
                ..Default::default()
            };

            // Simulate skip: remove from pending (e.g., already cached)
            state.pending_tables.remove("public.users");

            assert!(state.is_complete());
            assert!(!state.has_failures());
        }

        #[test]
        fn skip_with_prior_failures_still_complete() {
            let mut state = ErPreparationState {
                pending_tables: HashSet::from(["public.orders".to_string()]),
                fetching_tables: HashSet::new(),
                failed_tables: HashMap::from([("public.users".to_string(), "timeout".to_string())]),
                status: ErStatus::Waiting,
                total_tables: 2,
                target_tables: vec![],
                ..Default::default()
            };

            // Simulate skip: remove last pending (e.g., already cached)
            state.pending_tables.remove("public.orders");

            assert!(state.is_complete());
            assert!(state.has_failures());
        }
    }
}
