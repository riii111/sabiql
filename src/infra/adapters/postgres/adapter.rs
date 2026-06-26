pub struct PostgresAdapter {
    pub(in crate::adapters::postgres) timeout_secs: u64,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}
