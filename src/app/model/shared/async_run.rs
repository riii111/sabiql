/// Monotonic async run tracker for aggregate-owned freshness guards.
#[derive(Debug, Clone, Default)]
pub struct AsyncRun {
    run_id: u64,
    active_run_id: Option<u64>,
}

impl AsyncRun {
    /// Starts a new active run and returns its id.
    #[must_use]
    pub fn begin(&mut self) -> u64 {
        self.run_id += 1;
        self.active_run_id = Some(self.run_id);
        self.run_id
    }

    pub fn clear_active(&mut self) {
        self.active_run_id = None;
    }

    /// Returns the active run id, if this aggregate has an in-flight run.
    pub fn active_id(&self) -> Option<u64> {
        self.active_run_id
    }

    /// Returns the most recently allocated run id without reactivating it.
    pub fn last_id(&self) -> u64 {
        self.run_id
    }

    /// Checks whether a completion belongs to the currently active run.
    pub fn is_current(&self, run_id: u64) -> bool {
        self.active_run_id == Some(run_id)
    }
}
