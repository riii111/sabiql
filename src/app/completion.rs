use crate::app::sql_lexer::{SqlLexer, TokenCache};
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
    lexer: SqlLexer,
    #[allow(dead_code)] // Phase 3: differential tokenization
    token_cache: TokenCache,
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
            lexer: SqlLexer::new(),
            token_cache: TokenCache::new(),
        }
    }

    pub fn get_candidates(
        &self,
        content: &str,
        cursor_pos: usize,
        metadata: Option<&DatabaseMetadata>,
        table_detail: Option<&Table>,
    ) -> Vec<CompletionCandidate> {
        // Skip completion inside strings or comments
        if self.lexer.is_in_string_or_comment(content, cursor_pos) {
            return vec![];
        }

        let (current_token, context) = self.analyze(content, cursor_pos);

        match &context {
            CompletionContext::Keyword => self.keyword_candidates(&current_token),
            CompletionContext::Table => self.table_candidates(metadata, &current_token),
            CompletionContext::Column => self.column_candidates(table_detail, &current_token),
            CompletionContext::SchemaQualified(schema) => {
                self.schema_qualified_candidates(metadata, schema, &current_token)
            }
        }
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
            if let Some(pos) = self.find_keyword(before_upper, kw)
                && last_table_pos.is_none_or(|p| pos > p)
            {
                last_table_pos = Some(pos);
            }
        }

        for kw in keywords_column {
            if let Some(pos) = self.find_keyword(before_upper, kw)
                && last_column_pos.is_none_or(|p| pos > p)
            {
                last_column_pos = Some(pos);
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
        // Convert to char indices for safe multi-byte handling
        let chars: Vec<char> = text.chars().collect();
        let keyword_chars: Vec<char> = keyword.chars().collect();
        let keyword_len = keyword_chars.len();

        if chars.len() < keyword_len {
            return None;
        }

        // Search from end to start (rfind semantics)
        for start in (0..=chars.len() - keyword_len).rev() {
            // Check if keyword matches at this position
            if chars[start..start + keyword_len] != keyword_chars[..] {
                continue;
            }

            // Check word boundaries
            let before_ok = start == 0 || !Self::is_word_char(chars[start - 1]);
            let after_ok =
                start + keyword_len >= chars.len() || !Self::is_word_char(chars[start + keyword_len]);

            if before_ok && after_ok {
                return Some(start);
            }
        }
        None
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    fn keyword_candidates(&self, prefix: &str) -> Vec<CompletionCandidate> {
        let prefix_upper = prefix.to_uppercase();
        let mut candidates: Vec<_> = self.keywords
            .iter()
            .filter(|kw| prefix.is_empty() || kw.starts_with(&prefix_upper))
            .map(|kw| {
                let is_prefix_match = kw.starts_with(&prefix_upper);
                CompletionCandidate {
                    text: (*kw).to_string(),
                    kind: CompletionKind::Keyword,
                    detail: None,
                    score: if is_prefix_match { 100 } else { 10 },
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| {
            match b.score.cmp(&a.score) {
                std::cmp::Ordering::Equal => a.text.cmp(&b.text),
                other => other,
            }
        });

        candidates.into_iter().take(10).collect()
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
        let mut candidates: Vec<_> = metadata
            .tables
            .iter()
            .filter(|t| {
                prefix.is_empty()
                    || t.name.to_lowercase().starts_with(&prefix_lower)
                    || t.qualified_name().to_lowercase().starts_with(&prefix_lower)
            })
            .map(|t| {
                let name_lower = t.name.to_lowercase();
                let is_name_prefix = name_lower.starts_with(&prefix_lower);
                let is_qualified_prefix = t.qualified_name().to_lowercase().starts_with(&prefix_lower);
                let score = if is_name_prefix {
                    100
                } else if is_qualified_prefix {
                    50
                } else {
                    10
                };
                CompletionCandidate {
                    text: t.qualified_name(),
                    kind: CompletionKind::Table,
                    detail: t.row_count_estimate.map(|c| format!("~{} rows", c)),
                    score,
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| {
            match b.score.cmp(&a.score) {
                std::cmp::Ordering::Equal => a.text.cmp(&b.text),
                other => other,
            }
        });

        candidates.into_iter().take(10).collect()
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
        let mut candidates: Vec<_> = table
            .columns
            .iter()
            .filter(|c| prefix.is_empty() || c.name.to_lowercase().starts_with(&prefix_lower))
            .map(|c| {
                let is_prefix_match = c.name.to_lowercase().starts_with(&prefix_lower);
                let mut score = if is_prefix_match { 100 } else { 10 };

                // Boost PK columns
                if c.is_primary_key {
                    score += 50;
                }
                // Boost NOT NULL columns
                if !c.nullable {
                    score += 20;
                }

                CompletionCandidate {
                    text: c.name.clone(),
                    kind: CompletionKind::Column,
                    detail: Some(c.type_display()),
                    score,
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| {
            match b.score.cmp(&a.score) {
                std::cmp::Ordering::Equal => a.text.cmp(&b.text),
                other => other,
            }
        });

        candidates.into_iter().take(10).collect()
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

        let mut candidates: Vec<_> = metadata
            .tables
            .iter()
            .filter(|t| {
                t.schema.to_lowercase() == schema_lower
                    && (prefix.is_empty() || t.name.to_lowercase().starts_with(&prefix_lower))
            })
            .map(|t| {
                let is_prefix_match = t.name.to_lowercase().starts_with(&prefix_lower);
                CompletionCandidate {
                    text: t.name.clone(),
                    kind: CompletionKind::Table,
                    detail: t.row_count_estimate.map(|c| format!("~{} rows", c)),
                    score: if is_prefix_match { 100 } else { 10 },
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| {
            match b.score.cmp(&a.score) {
                std::cmp::Ordering::Equal => a.text.cmp(&b.text),
                other => other,
            }
        });

        candidates.into_iter().take(10).collect()
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

    mod word_boundary {
        use super::*;

        #[test]
        fn froma_does_not_match_from() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROMA", 14);

            // "FROMA" should be treated as a single token, not as FROM + A
            assert_eq!(token, "FROMA");
            // Since "FROMA" doesn't match FROM at word boundary,
            // the last valid keyword is SELECT, so context is Column
            assert_eq!(ctx, CompletionContext::Column);
        }

        #[test]
        fn from_with_space_matches_from() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECT * FROM ", 14);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::Table);
        }

        #[test]
        fn from_at_word_boundary_matches() {
            let e = engine();
            let (_token, ctx) = e.analyze("SELECT * FROM u", 15);

            // FROM is properly detected at word boundary
            assert_eq!(ctx, CompletionContext::Table);
        }

        #[test]
        fn selecta_does_not_match_select() {
            let e = engine();
            let (token, ctx) = e.analyze("SELECTA", 7);

            // "SELECTA" should be treated as a single token
            assert_eq!(token, "SELECTA");
            // Should not trigger column context
            assert_eq!(ctx, CompletionContext::Keyword);
        }
    }

    mod schema_qualified_limit {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        #[test]
        fn schema_qualified_candidates_limited_to_10() {
            let e = engine();

            // Create metadata with 15 tables in the same schema
            let mut tables = vec![];
            for i in 0..15 {
                tables.push(TableSummary::new(
                    "public".to_string(),
                    format!("table_{}", i),
                    Some(100),
                    false,
                ));
            }

            let mut metadata = DatabaseMetadata::new("test_db".to_string());
            metadata.tables = tables;

            let candidates = e.schema_qualified_candidates(
                Some(&metadata),
                "public",
                "table"
            );

            // Should be limited to 10 candidates
            assert_eq!(candidates.len(), 10);
            assert!(candidates.iter().all(|c| c.kind == CompletionKind::Table));
        }

        #[test]
        fn schema_qualified_candidates_with_empty_prefix() {
            let e = engine();

            let mut tables = vec![];
            for i in 0..5 {
                tables.push(TableSummary::new(
                    "myschema".to_string(),
                    format!("foo_{}", i),
                    None,
                    false,
                ));
            }

            let mut metadata = DatabaseMetadata::new("test_db".to_string());
            metadata.tables = tables;

            let candidates = e.schema_qualified_candidates(
                Some(&metadata),
                "myschema",
                ""
            );

            // Empty prefix should match all tables in schema
            assert_eq!(candidates.len(), 5);
        }
    }

    mod prefix_match_ranking {
        use super::*;
        use crate::domain::{Column, DatabaseMetadata, Table, TableSummary};

        #[test]
        fn keyword_prefix_match_ranked_first() {
            let e = engine();

            // Search with "S" - should prioritize SELECT over SET
            let candidates = e.keyword_candidates("S");

            assert!(!candidates.is_empty());
            // All returned candidates should start with "S"
            assert!(candidates.iter().all(|c| c.text.starts_with('S')));
            // Check that results are sorted
            let texts: Vec<_> = candidates.iter().map(|c| c.text.as_str()).collect();
            let mut sorted = texts.clone();
            sorted.sort();
            assert_eq!(texts, sorted);
        }

        #[test]
        fn table_name_prefix_ranked_over_qualified() {
            let e = engine();

            let mut metadata = DatabaseMetadata::new("test_db".to_string());
            metadata.tables = vec![
                TableSummary::new("users".to_string(), "data".to_string(), None, false),
                TableSummary::new("public".to_string(), "users".to_string(), None, false),
            ];

            let candidates = e.table_candidates(Some(&metadata), "u");

            // "public.users" should be ranked before "users.data"
            // because "users" table name starts with "u"
            assert_eq!(candidates.len(), 2);
            assert_eq!(candidates[0].text, "public.users");
        }

        #[test]
        fn column_prefix_match_sorted_alphabetically() {
            let e = engine();

            let table = Table {
                schema: "public".to_string(),
                name: "test".to_string(),
                columns: vec![
                    Column {
                        name: "user_name".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 1,
                    },
                    Column {
                        name: "user_id".to_string(),
                        data_type: "int".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: true,
                        is_unique: true,
                        comment: None,
                        ordinal_position: 2,
                    },
                ],
                primary_key: Some(vec!["user_id".to_string()]),
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                row_count_estimate: None,
                comment: None,
            };

            let candidates = e.column_candidates(Some(&table), "user");

            assert_eq!(candidates.len(), 2);
            // Should be sorted alphabetically among prefix matches
            assert_eq!(candidates[0].text, "user_id");
            assert_eq!(candidates[1].text, "user_name");
        }
    }

    mod string_and_comment_skip {
        use super::*;

        #[test]
        fn inside_single_quote_string_returns_empty() {
            let e = engine();

            let candidates = e.get_candidates("SELECT 'SEL", 11, None, None);

            assert!(candidates.is_empty());
        }

        #[test]
        fn inside_line_comment_returns_empty() {
            let e = engine();

            let candidates = e.get_candidates("-- SEL", 6, None, None);

            assert!(candidates.is_empty());
        }

        #[test]
        fn inside_block_comment_returns_empty() {
            let e = engine();

            let candidates = e.get_candidates("/* SEL", 6, None, None);

            assert!(candidates.is_empty());
        }

        #[test]
        fn inside_dollar_quote_returns_empty() {
            let e = engine();

            let candidates = e.get_candidates("SELECT $$SEL", 12, None, None);

            assert!(candidates.is_empty());
        }

        #[test]
        fn after_closed_string_returns_candidates() {
            let e = engine();

            let candidates = e.get_candidates("'value' SEL", 11, None, None);

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.text == "SELECT"));
        }

        #[test]
        fn after_closed_comment_returns_candidates() {
            let e = engine();

            let candidates = e.get_candidates("/* comment */ SEL", 17, None, None);

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.text == "SELECT"));
        }
    }

    mod score_ranking {
        use super::*;
        use crate::domain::{Column, Table};

        #[test]
        fn pk_column_returns_higher_score() {
            let e = engine();
            let table = Table {
                schema: "public".to_string(),
                name: "test".to_string(),
                columns: vec![
                    Column {
                        name: "name".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 1,
                    },
                    Column {
                        name: "id".to_string(),
                        data_type: "int".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: true,
                        is_unique: true,
                        comment: None,
                        ordinal_position: 2,
                    },
                ],
                primary_key: Some(vec!["id".to_string()]),
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                row_count_estimate: None,
                comment: None,
            };

            let candidates = e.column_candidates(Some(&table), "");

            assert_eq!(candidates[0].text, "id");
            assert!(candidates[0].score > candidates[1].score);
        }

        #[test]
        fn not_null_column_returns_higher_score() {
            let e = engine();
            let table = Table {
                schema: "public".to_string(),
                name: "test".to_string(),
                columns: vec![
                    Column {
                        name: "optional_field".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 1,
                    },
                    Column {
                        name: "required_field".to_string(),
                        data_type: "text".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 2,
                    },
                ],
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                row_count_estimate: None,
                comment: None,
            };

            let candidates = e.column_candidates(Some(&table), "");

            assert_eq!(candidates[0].text, "required_field");
            assert!(candidates[0].score > candidates[1].score);
        }
    }
}
