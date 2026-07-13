use std::borrow::Cow;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
        self.display_value_ref().into_owned()
    }

    #[must_use]
    pub fn display_value_ref(&self) -> Cow<'_, str> {
        match self {
            Self::Null => Cow::Borrowed("NULL"),
            Self::Text(value) | Self::SqlLiteral(value) if value.contains('\0') => {
                Cow::Owned(escape_display_text(value))
            }
            Self::Text(value) | Self::SqlLiteral(value) => Cow::Borrowed(value),
            Self::Blob(bytes) => Cow::Owned(blob_display_value(bytes)),
        }
    }

    #[must_use]
    pub fn display_width(&self) -> usize {
        match self {
            Self::Null => UnicodeWidthStr::width("NULL"),
            Self::Text(value) | Self::SqlLiteral(value) => display_width_of_first_line(value, true),
            Self::Blob(bytes) => blob_display_width(bytes),
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

fn blob_display_value(bytes: &[u8]) -> String {
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

fn display_width_of_first_line(value: &str, escape_nul: bool) -> usize {
    value
        .chars()
        .take_while(|&ch| ch != '\n')
        .map(|ch| {
            if escape_nul && ch == '\0' {
                2
            } else {
                UnicodeWidthChar::width(ch).unwrap_or(0)
            }
        })
        .sum()
}

fn blob_display_width(bytes: &[u8]) -> usize {
    let mut width = UnicodeWidthStr::width("BLOB (")
        + decimal_display_width(bytes.len())
        + UnicodeWidthStr::width(" bytes)");
    let preview_bytes = bytes.len().min(BLOB_PREVIEW_BYTES);
    if preview_bytes > 0 {
        width += 1 + preview_bytes * 2 + preview_bytes.saturating_sub(1);
        if bytes.len() > BLOB_PREVIEW_BYTES {
            width += UnicodeWidthStr::width(" ...");
        }
    }
    width
}

fn decimal_display_width(mut value: usize) -> usize {
    let mut width = 1;
    while value >= 10 {
        value /= 10;
        width += 1;
    }
    width
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySource {
    Preview,
    Adhoc,
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
        let row_count = values.len();
        Self {
            query,
            columns,
            rows: Vec::new(),
            values,
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
        if self.typed_values {
            for values in &mut self.values {
                let sentinel = values.pop();
                if sentinel == Some(QueryValue::Null) {
                    values.clear();
                }
            }
            self.values
                .retain(|values| values.len() == self.columns.len());
        } else {
            for (row, values) in self.rows.iter_mut().zip(&mut self.values) {
                let sentinel = values.pop();
                row.pop();
                if sentinel == Some(QueryValue::Null) {
                    values.clear();
                    row.clear();
                }
            }
            self.rows.retain(|row| row.len() == self.columns.len());
            self.values
                .retain(|values| values.len() == self.columns.len());
        }
        self.row_count = self.values.len();
        self
    }

    #[must_use]
    pub fn has_typed_values(&self) -> bool {
        self.typed_values
    }

    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    #[must_use]
    pub fn values(&self) -> &[Vec<QueryValue>] {
        &self.values
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    #[must_use]
    pub fn data_row_count(&self) -> usize {
        if self.typed_values {
            self.values.len()
        } else {
            self.rows.len()
        }
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

    #[must_use]
    pub fn display_value_ref_at(&self, row: usize, col: usize) -> Option<Cow<'_, str>> {
        if self.typed_values {
            self.value_at(row, col).map(QueryValue::display_value_ref)
        } else {
            self.rows
                .get(row)?
                .get(col)
                .map(|value| Cow::Borrowed(value.as_str()))
        }
    }

    #[must_use]
    pub fn display_value_at(&self, row: usize, col: usize) -> Option<String> {
        self.display_value_ref_at(row, col).map(Cow::into_owned)
    }

    #[must_use]
    pub fn display_width_at(&self, row: usize, col: usize) -> Option<usize> {
        if self.typed_values {
            self.value_at(row, col).map(QueryValue::display_width)
        } else {
            self.rows
                .get(row)?
                .get(col)
                .map(|value| display_width_of_first_line(value, false))
        }
    }

    #[must_use]
    pub fn display_row_at(&self, row: usize) -> Option<Vec<String>> {
        if self.typed_values {
            self.values
                .get(row)
                .map(|values| values.iter().map(QueryValue::display_value).collect())
        } else {
            self.rows.get(row).cloned()
        }
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
            assert_eq!(result.display_row_at(0), Some(vec!["1".to_string()]));
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
            assert_eq!(result.data_row_count(), 0);
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

    mod typed_values {
        use super::*;

        #[test]
        fn keeps_text_owned_only_by_typed_values() {
            let text = "a".repeat(4096);
            let result = QueryResult::success_with_values(
                "SELECT body".to_string(),
                vec!["body".to_string()],
                vec![vec![QueryValue::text(text.clone())]],
                0,
                QuerySource::Adhoc,
            );

            assert_eq!(result.data_row_count(), 1);
            assert_eq!(result.column_count(), 1);
            assert_eq!(
                result.display_value_ref_at(0, 0).as_deref(),
                Some(text.as_str())
            );
        }

        #[test]
        fn removes_sentinel_column_without_display_rows() {
            let result = QueryResult::success_with_values(
                "SELECT body, sentinel".to_string(),
                vec!["body".to_string(), "sentinel".to_string()],
                vec![vec![QueryValue::text("body"), QueryValue::text("sentinel")]],
                0,
                QuerySource::Adhoc,
            )
            .without_empty_result_sentinel();

            assert_eq!(result.columns, vec!["body"]);
            assert_eq!(result.values(), &[vec![QueryValue::text("body")]]);
            assert_eq!(result.display_row_at(0), Some(vec!["body".to_string()]));
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

        #[test]
        fn display_width_handles_large_nul_text_and_blob_without_display_materialization() {
            const SIZE: usize = 1024 * 1024;
            let text = format!("{}\0tail", "a".repeat(SIZE));
            let blob = vec![0xAB; SIZE];
            let result = QueryResult::success_with_values(
                "SELECT body, payload".to_string(),
                vec!["body".to_string(), "payload".to_string()],
                vec![vec![QueryValue::text(text), QueryValue::Blob(blob)]],
                0,
                QuerySource::Adhoc,
            );

            assert_eq!(result.display_width_at(0, 0), Some(SIZE + 6));
            assert_eq!(
                result.display_width_at(0, 1),
                Some("BLOB (1048576 bytes) AB AB AB AB AB AB AB AB ...".len())
            );
        }
    }
}
