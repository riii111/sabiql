use crate::app::state::{CompletionCandidate, CompletionKind};
use crate::domain::{DatabaseMetadata, Table};

/// Context detected from SQL text at cursor position
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    /// Start of statement or unknown context → keywords
    Keyword,
    /// After FROM/JOIN → table names
    Table,
    /// After SELECT/WHERE/ON or table reference → column names
    Column,
    /// After "schema." → tables in that schema
    SchemaQualified(String),
}

pub struct CompletionEngine {
    keywords: Vec<&'static str>,
}

impl Default for CompletionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionEngine {
    pub fn new() -> Self {
        Self {
            keywords: vec![
                "SELECT",
                "FROM",
                "WHERE",
                "JOIN",
                "LEFT",
                "RIGHT",
                "INNER",
                "OUTER",
                "CROSS",
                "ON",
                "AND",
                "OR",
                "NOT",
                "IN",
                "IS",
                "NULL",
                "TRUE",
                "FALSE",
                "LIKE",
                "ILIKE",
                "BETWEEN",
                "EXISTS",
                "CASE",
                "WHEN",
                "THEN",
                "ELSE",
                "END",
                "AS",
                "DISTINCT",
                "ORDER",
                "BY",
                "ASC",
                "DESC",
                "NULLS",
                "FIRST",
                "LAST",
                "GROUP",
                "HAVING",
                "LIMIT",
                "OFFSET",
                "UNION",
                "INTERSECT",
                "EXCEPT",
                "ALL",
                "INSERT",
                "INTO",
                "VALUES",
                "UPDATE",
                "SET",
                "DELETE",
                "CREATE",
                "DROP",
                "ALTER",
                "TABLE",
                "INDEX",
                "VIEW",
                "RETURNING",
                "WITH",
                "RECURSIVE",
                "COALESCE",
                "NULLIF",
                "CAST",
                "USING",
            ],
        }
    }

    pub fn get_candidates(
        &self,
        content: &str,
        cursor_pos: usize,
        metadata: Option<&DatabaseMetadata>,
        table_detail: Option<&Table>,
    ) -> Vec<CompletionCandidate> {
        let (current_token, context) = self.analyze(content, cursor_pos);

        let candidates = match &context {
            CompletionContext::Keyword => self.keyword_candidates(&current_token),
            CompletionContext::Table => self.table_candidates(metadata, &current_token),
            CompletionContext::Column => self.column_candidates(table_detail, &current_token),
            CompletionContext::SchemaQualified(schema) => {
                self.schema_qualified_candidates(metadata, schema, &current_token)
            }
        };

        // Fallback to keywords if context-specific candidates are empty
        if candidates.is_empty() && !matches!(context, CompletionContext::Keyword) {
            return self.keyword_candidates(&current_token);
        }
        candidates
    }

    pub fn current_token_len(&self, content: &str, cursor_pos: usize) -> usize {
        let before_cursor: String = content.chars().take(cursor_pos).collect();
        self.extract_current_token(&before_cursor).chars().count()
    }

    /// Analyze SQL content at cursor position to determine context and current token
    fn analyze(&self, content: &str, cursor_pos: usize) -> (String, CompletionContext) {
        let before_cursor: String = content.chars().take(cursor_pos).collect();
        let before_upper = before_cursor.to_uppercase();

        // Extract current token (word being typed)
        let current_token = self.extract_current_token(&before_cursor);

        // Check for schema-qualified context: "schema."
        if let Some(schema) = self.detect_schema_prefix(&before_cursor, &current_token) {
            return (current_token, CompletionContext::SchemaQualified(schema));
        }

        // Detect context from preceding keywords
        let context = self.detect_context(&before_upper);

        (current_token, context)
    }

    fn extract_current_token(&self, before_cursor: &str) -> String {
        before_cursor
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    }

    /// Check if cursor is after "schema." pattern
    fn detect_schema_prefix(&self, before_cursor: &str, current_token: &str) -> Option<String> {
        let prefix_end = before_cursor.len().saturating_sub(current_token.len());
        let prefix = &before_cursor[..prefix_end];

        if prefix.ends_with('.') {
            // Extract schema name before the dot
            let schema: String = prefix
                .trim_end_matches('.')
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect();

            if !schema.is_empty() {
                return Some(schema);
            }
        }
        None
    }

    fn detect_context(&self, before_upper: &str) -> CompletionContext {
        let keywords_table = ["FROM", "JOIN", "INTO", "UPDATE"];
        let keywords_column = ["SELECT", "WHERE", "ON", "SET", "AND", "OR", "BY"];

        let mut last_table_pos = None;
        let mut last_column_pos = None;

        for kw in keywords_table {
            if let Some(pos) = self.find_keyword(before_upper, kw) {
                if last_table_pos.map_or(true, |p| pos > p) {
                    last_table_pos = Some(pos);
                }
            }
        }

        for kw in keywords_column {
            if let Some(pos) = self.find_keyword(before_upper, kw) {
                if last_column_pos.map_or(true, |p| pos > p) {
                    last_column_pos = Some(pos);
                }
            }
        }

        match (last_table_pos, last_column_pos) {
            (Some(t), Some(c)) if t > c => CompletionContext::Table,
            (Some(t), None) if t > 0 => CompletionContext::Table,
            (_, Some(_)) => CompletionContext::Column,
            _ => CompletionContext::Keyword,
        }
    }

    fn find_keyword(&self, text: &str, keyword: &str) -> Option<usize> {
        let mut search_from = text.len();
        while search_from > 0 {
            let slice = &text[..search_from];
            let Some(pos) = slice.rfind(keyword) else {
                return None;
            };

            let before_ok = pos == 0
                || !text.chars().nth(pos - 1).map_or(false, |c| c.is_alphanumeric() || c == '_');
            let after_pos = pos + keyword.len();
            let after_ok = after_pos >= text.len()
                || !text.chars().nth(after_pos).map_or(false, |c| c.is_alphanumeric() || c == '_');

            if before_ok && after_ok {
                return Some(pos);
            }
            search_from = pos;
        }
        None
    }

    fn keyword_candidates(&self, prefix: &str) -> Vec<CompletionCandidate> {
        let prefix_upper = prefix.to_uppercase();
        self.keywords
            .iter()
            .filter(|kw| prefix.is_empty() || kw.starts_with(&prefix_upper))
            .take(10)
            .map(|kw| CompletionCandidate {
                text: (*kw).to_string(),
                kind: CompletionKind::Keyword,
                detail: None,
            })
            .collect()
    }

    fn table_candidates(
        &self,
        metadata: Option<&DatabaseMetadata>,
        prefix: &str,
    ) -> Vec<CompletionCandidate> {
        let Some(metadata) = metadata else {
            return vec![];
        };

        let prefix_lower = prefix.to_lowercase();
        metadata
            .tables
            .iter()
            .filter(|t| {
                prefix.is_empty()
                    || t.name.to_lowercase().starts_with(&prefix_lower)
                    || t.qualified_name().to_lowercase().starts_with(&prefix_lower)
            })
            .take(10)
            .map(|t| CompletionCandidate {
                text: t.qualified_name(),
                kind: CompletionKind::Table,
                detail: t.row_count_estimate.map(|c| format!("~{} rows", c)),
            })
            .collect()
    }

    fn column_candidates(
        &self,
        table_detail: Option<&Table>,
        prefix: &str,
    ) -> Vec<CompletionCandidate> {
        let Some(table) = table_detail else {
            return vec![];
        };

        let prefix_lower = prefix.to_lowercase();
        table
            .columns
            .iter()
            .filter(|c| prefix.is_empty() || c.name.to_lowercase().starts_with(&prefix_lower))
            .take(10)
            .map(|c| CompletionCandidate {
                text: c.name.clone(),
                kind: CompletionKind::Column,
                detail: Some(c.type_display()),
            })
            .collect()
    }

    fn schema_qualified_candidates(
        &self,
        metadata: Option<&DatabaseMetadata>,
        schema: &str,
        prefix: &str,
    ) -> Vec<CompletionCandidate> {
        let Some(metadata) = metadata else {
            return vec![];
        };

        let schema_lower = schema.to_lowercase();
        let prefix_lower = prefix.to_lowercase();

        metadata
            .tables
            .iter()
            .filter(|t| {
                t.schema.to_lowercase() == schema_lower
                    && t.name.to_lowercase().starts_with(&prefix_lower)
            })
            .take(10)
            .map(|t| CompletionCandidate {
                text: t.name.clone(),
                kind: CompletionKind::Table,
                detail: t.row_count_estimate.map(|c| format!("~{} rows", c)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> CompletionEngine {
        CompletionEngine::new()
    }

    mod context_detection {
        use super::*;

        #[test]
        fn empty_input_returns_keyword_context() {
            let e = engine();
            let (token, ctx) = e.analyze("", 0);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::Keyword);
        }

        #[test]
        fn after_select_returns_column_context() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT ", 7);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::Column);
        }

        #[test]
        fn after_from_returns_table_context() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROM ", 14);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::Table);
        }

        #[test]
        fn after_join_returns_table_context() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROM users JOIN ", 25);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::Table);
        }

        #[test]
        fn after_where_returns_column_context() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROM users WHERE ", 26);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::Column);
        }

        #[test]
        fn partial_token_is_extracted() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROM us", 16);

            assert_eq!(token, "us");
            assert_eq!(ctx, CompletionContext::Table);
        }

        #[test]
        fn schema_dot_returns_schema_qualified() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROM public.", 21);

            assert_eq!(token, "");
            assert_eq!(
                ctx,
                CompletionContext::SchemaQualified("public".to_string())
            );
        }

        #[test]
        fn schema_dot_with_partial_table() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROM public.us", 23);

            assert_eq!(token, "us");
            assert_eq!(
                ctx,
                CompletionContext::SchemaQualified("public".to_string())
            );
        }
    }

    mod keyword_completion {
        use super::*;

        #[test]
        fn empty_prefix_returns_all_keywords() {
            let e = engine();
            let candidates = e.keyword_candidates("");

            assert!(!candidates.is_empty());
            assert!(candidates.iter().all(|c| c.kind == CompletionKind::Keyword));
        }

        #[test]
        fn sel_prefix_returns_select() {
            let e = engine();
            let candidates = e.keyword_candidates("SEL");

            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].text, "SELECT");
        }

        #[test]
        fn case_insensitive_matching() {
            let e = engine();
            let candidates = e.keyword_candidates("sel");

            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].text, "SELECT");
        }
    }
}
