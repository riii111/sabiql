use std::time::Instant;

/// Represents the source of a query result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySource {
    /// Automatic preview query (SELECT * LIMIT N) triggered by table selection
    Preview,
    /// Ad-hoc query executed from SQL Modal
    Adhoc,
}

/// Represents the result of a SQL query execution
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// The SQL query that was executed
    pub query: String,
    /// Column names from the result set
    pub columns: Vec<String>,
    /// Row data as strings (each inner Vec represents a row)
    pub rows: Vec<Vec<String>>,
    /// Total number of rows returned
    pub row_count: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// When the query was executed
    pub executed_at: Instant,
    /// Source of the query (Preview or Adhoc)
    pub source: QuerySource,
    /// Error message if the query failed
    pub error: Option<String>,
}

impl QueryResult {
    /// Create a successful query result
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

    /// Create an error query result
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

    /// Check if this result represents an error
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Get a display string for the row count
    pub fn row_count_display(&self) -> String {
        if self.row_count == 1 {
            "1 row".to_string()
        } else {
            format!("{} rows", self.row_count)
        }
    }

    /// Get the age of this result in seconds
    pub fn age_seconds(&self) -> u64 {
        self.executed_at.elapsed().as_secs()
    }
}
