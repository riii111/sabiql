use std::collections::{HashMap, HashSet};

use crate::model::shared::async_run::AsyncRun;

#[derive(Debug, Clone, Default)]
pub struct TablePrefetchState {
    in_flight: HashSet<String>,
    failures: HashMap<String, String>,
    tracks_er: bool,
    run: AsyncRun,
}

impl TablePrefetchState {
    pub fn reset_prefetch(&mut self) {
        self.in_flight.clear();
        self.failures.clear();
        self.tracks_er = false;
        self.run.clear_active();
    }

    pub fn begin(&mut self, tables: impl IntoIterator<Item = String>, tracks_er: bool) -> u64 {
        self.in_flight = tables.into_iter().collect();
        self.tracks_er = tracks_er;
        self.run.begin()
    }

    pub fn finish(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.in_flight)
    }

    pub fn clear_failures(&mut self) {
        self.failures.clear();
    }

    pub fn fail(&mut self, table: String, error: String) {
        self.failures.insert(table, error);
    }

    pub fn failed_prefetch(&self, table: &str) -> bool {
        self.failures.contains_key(table)
    }

    pub fn in_flight(&self) -> &HashSet<String> {
        &self.in_flight
    }

    pub fn prefetch_tracks_er(&self) -> bool {
        self.tracks_er
    }

    pub fn invalidate_prefetch(&mut self) {
        self.in_flight.clear();
        self.tracks_er = false;
        self.run.clear_active();
    }

    pub fn failed_table_errors(&self) -> Vec<(String, String)> {
        self.failures
            .iter()
            .map(|(table, error)| (table.clone(), error.clone()))
            .collect()
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    pub fn failed_prefetch_count(&self) -> usize {
        self.failures.len()
    }

    pub fn prefetch_in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    pub fn active_prefetch_run_id(&self) -> Option<u64> {
        self.run.active_id()
    }

    pub fn is_current_prefetch_run(&self, run_id: u64) -> bool {
        self.run.is_current(run_id)
    }

    pub fn is_complete(&self) -> bool {
        self.in_flight.is_empty()
    }
}
