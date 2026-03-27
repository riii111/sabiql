use std::time::Instant;

use super::CommandTag;

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
    pub command_tag: Option<CommandTag>,
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
            command_tag: None,
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
            command_tag: None,
        }
    }

    #[must_use]
    pub fn with_command_tag(mut self, tag: CommandTag) -> Self {
        self.command_tag = Some(tag);
        self
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod success {
        use super::*;

        #[test]
        fn creates_with_correct_fields() {
            let result = QueryResult::success(
                "SELECT 1".to_string(),
                vec!["id".to_string()],
                vec![vec!["1".to_string()]],
                42,
                QuerySource::Adhoc,
            );

            assert_eq!(result.query, "SELECT 1");
            assert_eq!(result.columns, vec!["id"]);
            assert_eq!(result.rows, vec![vec!["1"]]);
            assert_eq!(result.row_count, 1);
            assert_eq!(result.execution_time_ms, 42);
            assert_eq!(result.source, QuerySource::Adhoc);
            assert!(result.error.is_none());
            assert!(!result.is_error());
            assert!(result.command_tag.is_none());
        }

        #[test]
        fn row_count_matches_rows_len() {
            let result = QueryResult::success(
                "SELECT".to_string(),
                vec![],
                vec![vec![], vec![], vec![]],
                0,
                QuerySource::Preview,
            );

            assert_eq!(result.row_count, 3);
        }
    }

    mod error {
        use super::*;

        #[test]
        fn creates_with_empty_rows_and_error_message() {
            let result = QueryResult::error(
                "BAD SQL".to_string(),
                "syntax error".to_string(),
                10,
                QuerySource::Adhoc,
            );

            assert!(result.is_error());
            assert_eq!(result.error.as_deref(), Some("syntax error"));
            assert!(result.columns.is_empty());
            assert!(result.rows.is_empty());
            assert_eq!(result.row_count, 0);
        }
    }

    mod builder {
        use super::*;

        #[test]
        fn with_command_tag_sets_tag() {
            let result =
                QueryResult::success("SELECT".to_string(), vec![], vec![], 0, QuerySource::Adhoc)
                    .with_command_tag(CommandTag::Select(1));

            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }
    }

    mod row_count_display {
        use super::*;

        #[rstest]
        #[case(0, "0 rows")]
        #[case(1, "1 row")]
        #[case(5, "5 rows")]
        fn returns_expected(#[case] count: usize, #[case] expected: &str) {
            let mut result =
                QueryResult::success("SELECT".to_string(), vec![], vec![], 0, QuerySource::Adhoc);
            result.row_count = count;

            assert_eq!(result.row_count_display(), expected);
        }
    }
}
