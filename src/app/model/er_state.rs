use std::collections::HashMap;

use crate::model::shared::async_run::AsyncRun;
use crate::model::table_prefetch::TablePrefetchState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErStatus {
    #[default]
    Idle,
    Waiting,
    Rendering,
}

#[derive(Debug, Clone, Default)]
pub struct ErPreparationState {
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
}

impl ErPreparationState {
    pub fn status(&self) -> ErStatus {
        self.status
    }

    pub fn is_waiting(&self) -> bool {
        self.status == ErStatus::Waiting
    }

    pub fn progress(&self, prefetch: &TablePrefetchState) -> ErPreparationProgress {
        let failed = prefetch.failed_prefetch_count();
        let remaining = prefetch.pending_prefetch_count() + prefetch.prefetch_in_flight_count();
        let cached = self.total_tables.saturating_sub(remaining + failed);
        ErPreparationProgress {
            cached,
            total: self.total_tables,
        }
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

    pub fn begin_all_prefetch<I>(&mut self, tables: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.total_tables = tables.into_iter().count();
        self.fk_expanded = true;
    }

    pub fn begin_scoped_prefetch(&mut self, tables: impl AsRef<[String]>) {
        let tables = tables.as_ref();
        self.fk_expanded = false;
        self.seed_tables = tables.to_vec();
        self.total_tables = tables.len();
    }

    pub fn mark_idle(&mut self) {
        self.status = ErStatus::Idle;
    }

    pub fn is_current_run(&self, run_id: u64) -> bool {
        self.run.is_current(run_id)
    }

    pub fn invalidate_run(&mut self) {
        self.status = ErStatus::Idle;
        self.run.clear_active();
    }

    pub fn run_id(&self) -> u64 {
        self.run.last_id()
    }

    pub fn can_generate_from_cache(&self) -> bool {
        matches!(self.status, ErStatus::Idle | ErStatus::Waiting)
    }

    pub fn reset(&mut self) {
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

    #[test]
    fn all_prefetch_records_total_and_marks_fk_expanded() {
        let mut state = ErPreparationState::default();

        state.begin_all_prefetch(vec![
            "public.users".to_string(),
            "public.orders".to_string(),
        ]);

        assert_eq!(state.total_tables, 2);
        assert!(state.fk_expanded);
    }

    #[test]
    fn scoped_prefetch_records_seed_tables_without_prefetch_collections() {
        let mut state = ErPreparationState {
            fk_expanded: true,
            ..Default::default()
        };
        let tables = vec!["public.users".to_string(), "public.orders".to_string()];

        state.begin_scoped_prefetch(&tables);

        assert_eq!(state.total_tables, 2);
        assert!(!state.fk_expanded);
        assert_eq!(state.seed_tables, tables);
    }

    #[test]
    fn reset_clears_er_state_but_preserves_run_counter() {
        let mut state = ErPreparationState {
            status: ErStatus::Waiting,
            total_tables: 3,
            target_tables: vec!["public.users".to_string()],
            seed_tables: vec!["public.users".to_string()],
            fk_expanded: true,
            last_signatures: HashMap::from([("public.users".to_string(), "sig".to_string())]),
            ..Default::default()
        };
        set_active_run_id(&mut state, 5);

        state.reset();

        assert_eq!(state.status, ErStatus::Idle);
        assert_eq!(state.total_tables, 0);
        assert!(state.target_tables.is_empty());
        assert!(state.seed_tables.is_empty());
        assert!(!state.fk_expanded);
        assert!(state.last_signatures.is_empty());
        assert_eq!(state.run_id(), 5);
    }
}
