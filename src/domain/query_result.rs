use std::borrow::Cow;

use super::{CommandTag, DatabaseDiagnostic};

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
    use std::fmt::Write as _;

    let preview_bytes = bytes.len().min(BLOB_PREVIEW_BYTES);
    let mut display = String::with_capacity(32 + preview_bytes * 3);
    let _ = write!(display, "BLOB ({} bytes)", bytes.len());
    if preview_bytes > 0 {
        display.push(' ');
        for (index, byte) in bytes.iter().take(preview_bytes).enumerate() {
            if index > 0 {
                display.push(' ');
            }
            let _ = write!(display, "{byte:02X}");
        }
        if bytes.len() > BLOB_PREVIEW_BYTES {
            display.push_str(" ...");
        }
    }
    display
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySource {
    Preview,
    Adhoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitRowIdentity {
    columns: Vec<String>,
    values: Vec<Vec<QueryValue>>,
}

impl ExplicitRowIdentity {
    #[must_use]
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    #[must_use]
    pub fn values(&self) -> &[Vec<QueryValue>] {
        &self.values
    }

    #[must_use]
    pub fn values_for_row(&self, row: usize) -> Option<&[QueryValue]> {
        self.values.get(row).map(Vec::as_slice)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefreshScope {
    None,
    Data,
    Metadata,
}

impl RefreshScope {
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        self.max(other)
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
    pub refresh_scope: RefreshScope,
    pub mysql_diagnostics: Vec<DatabaseDiagnostic>,
    values: Vec<Vec<QueryValue>>,
    explicit_row_identity: Option<ExplicitRowIdentity>,
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
            .into_iter()
            .map(|row| row.into_iter().map(QueryValue::Text).collect())
            .collect();
        Self {
            query,
            columns,
            values,
            explicit_row_identity: None,
            row_count,
            typed_values: false,
            execution_time_ms,
            source,
            error: None,
            command_tag: None,
            refresh_scope: RefreshScope::None,
            mysql_diagnostics: Vec::new(),
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
            values,
            explicit_row_identity: None,
            row_count,
            typed_values: true,
            execution_time_ms,
            source,
            error: None,
            command_tag: None,
            refresh_scope: RefreshScope::None,
            mysql_diagnostics: Vec::new(),
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
            values: Vec::new(),
            explicit_row_identity: None,
            row_count: 0,
            typed_values: false,
            execution_time_ms,
            source,
            error: Some(error),
            command_tag: None,
            refresh_scope: RefreshScope::None,
            mysql_diagnostics: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_command_tag(mut self, tag: CommandTag) -> Self {
        self.refresh_scope = tag.refresh_scope();
        self.command_tag = Some(tag);
        self
    }

    #[must_use]
    pub fn with_refresh_scope(mut self, refresh_scope: RefreshScope) -> Self {
        self.refresh_scope = refresh_scope;
        self
    }

    #[must_use]
    pub fn with_mysql_diagnostics(mut self, diagnostics: Vec<DatabaseDiagnostic>) -> Self {
        self.mysql_diagnostics = diagnostics;
        self
    }

    #[must_use]
    pub fn with_row_count(mut self, row_count: usize) -> Self {
        self.row_count = row_count;
        self
    }

    #[must_use]
    pub fn with_explicit_row_identity(
        mut self,
        columns: Vec<String>,
        values: Vec<Vec<QueryValue>>,
    ) -> Self {
        self.explicit_row_identity = Some(ExplicitRowIdentity { columns, values });
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
        for values in &mut self.values {
            let sentinel = values.pop();
            if sentinel == Some(QueryValue::Null) {
                values.clear();
            }
        }
        self.values
            .retain(|values| values.len() == self.columns.len());
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
    pub fn explicit_row_identity(&self) -> Option<&ExplicitRowIdentity> {
        self.explicit_row_identity.as_ref()
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    #[must_use]
    pub fn data_row_count(&self) -> usize {
        self.values.len()
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
            self.value_at(row, col)
                .and_then(|value| value.as_str().map(Cow::Borrowed))
        }
    }

    #[must_use]
    pub fn display_value_at(&self, row: usize, col: usize) -> Option<String> {
        self.display_value_ref_at(row, col).map(Cow::into_owned)
    }

    #[must_use]
    pub fn display_row_at(&self, row: usize) -> Option<Vec<String>> {
        if self.typed_values {
            self.values
                .get(row)
                .map(|values| values.iter().map(QueryValue::display_value).collect())
        } else {
            self.values.get(row).map(|values| {
                values
                    .iter()
                    .filter_map(QueryValue::as_str)
                    .map(str::to_string)
                    .collect()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        fn preserves_text_values_and_untyped_display() {
            let text = "a\0bc".to_string();
            let result = QueryResult::success(
                "SELECT body".to_string(),
                vec!["body".to_string()],
                vec![vec![text.clone()]],
                0,
                QuerySource::Adhoc,
            );

            assert_eq!(result.values(), &[vec![QueryValue::text(text.clone())]]);
            assert_eq!(
                result.display_value_ref_at(0, 0).as_deref(),
                Some(text.as_str())
            );
            assert_eq!(result.display_row_at(0), Some(vec![text]));
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
        use crate::DiagnosticLevel;

        #[test]
        fn with_command_tag_sets_tag() {
            let result =
                QueryResult::success("SELECT".to_string(), vec![], vec![], 0, QuerySource::Adhoc)
                    .with_command_tag(CommandTag::Select(1));

            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[test]
        fn with_mysql_diagnostics_keeps_success_status_and_details() {
            let result = QueryResult::success(
                "INSERT IGNORE".to_string(),
                vec![],
                vec![],
                0,
                QuerySource::Adhoc,
            )
            .with_mysql_diagnostics(vec![DatabaseDiagnostic {
                level: DiagnosticLevel::Warning,
                code: 1062,
                message: "duplicate".to_string(),
            }]);

            assert!(!result.is_error());
            assert_eq!(result.mysql_diagnostics[0].code, 1062);
        }
    }

    mod typed_values {
        use super::*;

        #[test]
        fn stores_typed_text_without_a_second_display_row_buffer() {
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

        #[test]
        fn explicit_identity_is_not_part_of_display_rows_or_column_count() {
            let result = QueryResult::success_with_values(
                "SELECT payload".to_string(),
                vec!["payload".to_string()],
                vec![vec![QueryValue::text("visible")]],
                0,
                QuerySource::Preview,
            )
            .with_explicit_row_identity(
                vec!["id".to_string()],
                vec![vec![QueryValue::SqlLiteral("42".to_string())]],
            );

            assert_eq!(result.column_count(), 1);
            assert_eq!(result.display_row_at(0), Some(vec!["visible".to_string()]));
            assert_eq!(
                result
                    .explicit_row_identity()
                    .and_then(|identity| identity.values_for_row(0)),
                Some([QueryValue::SqlLiteral("42".to_string())].as_slice())
            );
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
