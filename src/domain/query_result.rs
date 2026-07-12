use super::CommandTag;

const BLOB_PREVIEW_BYTES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryValue {
    Null,
    Text(String),
    Blob(Vec<u8>),
    /// Unquoted SQL literal emitted by a trusted database adapter parser.
    SqlLiteral(String),
}

impl QueryValue {
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    #[must_use]
    pub fn display_value(&self) -> String {
        match self {
            Self::Null => "NULL".to_string(),
            Self::Text(value) | Self::SqlLiteral(value) => escape_display_text(value),
            Self::Blob(bytes) => {
                let preview = bytes
                    .iter()
                    .take(BLOB_PREVIEW_BYTES)
                    .map(|byte| format!("{byte:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if preview.is_empty() {
                    "BLOB (0 bytes)".to_string()
                } else if bytes.len() > BLOB_PREVIEW_BYTES {
                    format!("BLOB ({} bytes) {preview} ...", bytes.len())
                } else {
                    format!("BLOB ({} bytes) {preview}", bytes.len())
                }
            }
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(value) | Self::SqlLiteral(value) => Some(value),
            Self::Null | Self::Blob(_) => None,
        }
    }

    #[must_use]
    pub fn copy_value(&self) -> String {
        match self {
            Self::Null => "NULL".to_string(),
            Self::Text(value) | Self::SqlLiteral(value) => value.clone(),
            Self::Blob(bytes) => {
                let hex =
                    bytes
                        .iter()
                        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
                            use std::fmt::Write as _;
                            let _ = write!(hex, "{byte:02X}");
                            hex
                        });
                format!("X'{hex}'")
            }
        }
    }
}

fn escape_display_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '\0' {
            escaped.push_str("\\0");
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySource {
    Preview,
    Adhoc,
}

#[derive(Debug, Clone, Default)]
struct HiddenQueryColumns {
    columns: Vec<String>,
    values: Vec<Vec<QueryValue>>,
}

impl HiddenQueryColumns {
    fn value_at(&self, row: usize, column: &str) -> Option<&QueryValue> {
        let col = self.columns.iter().position(|name| name == column)?;
        self.values.get(row)?.get(col)
    }
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub query: String,
    pub columns: Vec<String>,
    pub execution_time_ms: u64,
    pub source: QuerySource,
    pub error: Option<String>,
    pub command_tag: Option<CommandTag>,
    rows: Vec<Vec<String>>,
    values: Vec<Vec<QueryValue>>,
    hidden: HiddenQueryColumns,
    row_count: usize,
    typed_values: bool,
}

impl QueryResult {
    #[must_use]
    pub fn success(
        query: String,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        execution_time_ms: u64,
        source: QuerySource,
    ) -> Self {
        let row_count = rows.len();
        let values = rows
            .iter()
            .map(|row| row.iter().cloned().map(QueryValue::Text).collect())
            .collect();
        Self {
            query,
            columns,
            rows,
            values,
            hidden: HiddenQueryColumns::default(),
            row_count,
            typed_values: false,
            execution_time_ms,
            source,
            error: None,
            command_tag: None,
        }
    }

    #[must_use]
    pub fn success_with_values(
        query: String,
        columns: Vec<String>,
        values: Vec<Vec<QueryValue>>,
        execution_time_ms: u64,
        source: QuerySource,
    ) -> Self {
        let rows = values
            .iter()
            .map(|row| row.iter().map(QueryValue::display_value).collect())
            .collect();
        let row_count = values.len();
        Self {
            query,
            columns,
            rows,
            values,
            hidden: HiddenQueryColumns::default(),
            row_count,
            typed_values: true,
            execution_time_ms,
            source,
            error: None,
            command_tag: None,
        }
    }

    #[must_use]
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
            values: Vec::new(),
            hidden: HiddenQueryColumns::default(),
            row_count: 0,
            typed_values: false,
            execution_time_ms,
            source,
            error: Some(error),
            command_tag: None,
        }
    }

    #[must_use]
    pub fn with_command_tag(mut self, tag: CommandTag) -> Self {
        self.command_tag = Some(tag);
        self
    }

    #[must_use]
    pub fn with_row_count(mut self, row_count: usize) -> Self {
        self.row_count = row_count;
        self
    }

    #[must_use]
    pub fn with_columns_if_empty(mut self, columns: Vec<String>) -> Self {
        if self.columns.is_empty() {
            self.columns = columns;
        }
        self
    }

    #[must_use]
    pub fn without_empty_result_sentinel(mut self) -> Self {
        self.columns.pop();
        for (row, values) in self.rows.iter_mut().zip(&mut self.values) {
            let sentinel = values.pop();
            row.pop();
            if sentinel == Some(QueryValue::Null) {
                values.clear();
                row.clear();
            }
        }
        self.rows.retain(|row| row.len() == self.columns.len());
        self.values.retain(|row| row.len() == self.columns.len());
        self.row_count = self.values.len();
        self
    }

    #[must_use]
    pub fn has_typed_values(&self) -> bool {
        self.typed_values
    }

    #[must_use]
    pub fn with_first_column_hidden(mut self, hidden_column: String) -> Self {
        if self.columns.is_empty() {
            return self;
        }

        self.columns.remove(0);
        for row in &mut self.rows {
            if !row.is_empty() {
                row.remove(0);
            }
        }

        let mut hidden_values = Vec::with_capacity(self.values.len());
        for row in &mut self.values {
            if row.is_empty() {
                hidden_values.push(Vec::new());
            } else {
                hidden_values.push(vec![row.remove(0)]);
            }
        }
        self.hidden = HiddenQueryColumns {
            columns: vec![hidden_column],
            values: hidden_values,
        };
        self
    }

    #[must_use]
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    #[must_use]
    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }

    #[must_use]
    pub fn values(&self) -> &[Vec<QueryValue>] {
        &self.values
    }

    #[must_use]
    pub fn hidden_value_at(&self, row: usize, column: &str) -> Option<&QueryValue> {
        self.hidden.value_at(row, column)
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    #[must_use]
    pub fn row_count_display(&self) -> String {
        if self.row_count == 1 {
            "1 row".to_string()
        } else {
            format!("{} rows", self.row_count)
        }
    }

    #[must_use]
    pub fn value_at(&self, row: usize, col: usize) -> Option<&QueryValue> {
        self.values.get(row)?.get(col)
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
            assert_eq!(result.rows(), vec![vec!["1"]]);
            assert_eq!(result.row_count(), 1);
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

            assert_eq!(result.row_count(), 3);
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
            assert!(result.rows().is_empty());
            assert_eq!(result.row_count(), 0);
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
        fn formats_row_count_display(#[case] count: usize, #[case] expected: &str) {
            let result =
                QueryResult::success("SELECT".to_string(), vec![], vec![], 0, QuerySource::Adhoc)
                    .with_row_count(count);

            assert_eq!(result.row_count_display(), expected);
        }
    }

    mod nul_text {
        use super::*;

        #[test]
        fn display_value_escapes_embedded_nul_byte() {
            assert_eq!(QueryValue::text("a\0bc").display_value(), "a\\0bc");
        }
    }
}
