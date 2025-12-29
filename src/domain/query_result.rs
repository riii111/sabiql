use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySource {
    Preview,
    Adhoc,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub query: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub execution_time_ms: u64,
    pub executed_at: Instant,
    pub source: QuerySource,
    pub error: Option<String>,
}

impl QueryResult {
    pub fn success(
        query: String,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        execution_time_ms: u64,
        source: QuerySource,
    ) -> Self {
        let row_count = rows.len();
        Self {
            query,
            columns,
            rows,
            row_count,
            execution_time_ms,
            executed_at: Instant::now(),
            source,
            error: None,
        }
    }

    pub fn error(
        query: String,
        error: String,
        execution_time_ms: u64,
        source: QuerySource,
    ) -> Self {
        Self {
            query,
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            execution_time_ms,
            source,
            executed_at: Instant::now(),
            error: Some(error),
        }
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    pub fn row_count_display(&self) -> String {
        if self.row_count == 1 {
            "1 row".to_string()
        } else {
            format!("{} rows", self.row_count)
        }
    }

    pub fn age_seconds(&self) -> u64 {
        self.executed_at.elapsed().as_secs()
    }
}
