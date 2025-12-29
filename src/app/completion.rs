use std::collections::HashMap;

use crate::app::sql_lexer::{SqlContext, SqlLexer, TableReference, Token, TokenCache, TokenKind};
use crate::app::state::{CompletionCandidate, CompletionKind};
use crate::domain::{DatabaseMetadata, Table};

const COMPLETION_MAX_CANDIDATES: usize = 20;

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
    /// After "alias." → columns of that aliased table
    AliasColumn(String),
    /// CTE or table names (in FROM clause with CTEs defined)
    CteOrTable,
}

pub struct CompletionEngine {
    keywords: Vec<&'static str>,
    lexer: SqlLexer,
    #[allow(dead_code)] // Phase 3: differential tokenization
    token_cache: TokenCache,
    table_detail_cache: HashMap<String, Table>,
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
            table_detail_cache: HashMap::new(),
        }
    }

    #[allow(dead_code)] // Phase 4: called from main.rs when table details are fetched
    pub fn cache_table_detail(&mut self, qualified_name: String, table: Table) {
        self.table_detail_cache.insert(qualified_name, table);
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

        // Build SQL context for alias resolution
        let tokens = self.lexer.tokenize(content, content.len(), None);
        let sql_context = self.lexer.build_context(&tokens, cursor_pos);

        let (current_token, context) =
            self.analyze_with_context(content, cursor_pos, &sql_context, &tokens);

        let candidates = match &context {
            CompletionContext::Keyword => self.keyword_candidates(&current_token),
            CompletionContext::Table => self.table_candidates(metadata, &current_token),
            CompletionContext::Column => self.column_candidates(table_detail, &current_token),
            CompletionContext::SchemaQualified(schema) => {
                self.schema_qualified_candidates(metadata, schema, &current_token)
            }
            CompletionContext::AliasColumn(alias) => {
                self.alias_column_candidates(alias, &sql_context, metadata, &current_token)
            }
            CompletionContext::CteOrTable => {
                self.cte_or_table_candidates(&sql_context, metadata, &current_token)
            }
        };

        // Fallback to keywords if context-specific candidates are empty
        if candidates.is_empty() && context != CompletionContext::Keyword {
            return self.keyword_candidates(&current_token);
        }

        candidates
    }

    pub fn current_token_len(&self, content: &str, cursor_pos: usize) -> usize {
        let before_cursor: String = content.chars().take(cursor_pos).collect();
        self.extract_current_token(&before_cursor).chars().count()
    }

    #[allow(dead_code)] // Keep for backwards compatibility in tests
    fn analyze(&self, content: &str, cursor_pos: usize) -> (String, CompletionContext) {
        let tokens = self.lexer.tokenize(content, cursor_pos, None);
        let sql_context = SqlContext::default();
        self.analyze_with_context(content, cursor_pos, &sql_context, &tokens)
    }

    fn analyze_with_context(
        &self,
        content: &str,
        cursor_pos: usize,
        sql_context: &SqlContext,
        tokens: &[Token],
    ) -> (String, CompletionContext) {
        let before_cursor: String = content.chars().take(cursor_pos).collect();

        // Extract current token (word being typed)
        let current_token = self.extract_current_token(&before_cursor);

        // Check for alias.column pattern first (e.g., "u." or "u.na")
        if let Some(alias) = self.detect_alias_prefix(&before_cursor, &current_token, sql_context) {
            return (current_token, CompletionContext::AliasColumn(alias));
        }

        // Check for schema-qualified context: "schema."
        if let Some(schema) = self.detect_schema_prefix(&before_cursor, &current_token) {
            return (current_token, CompletionContext::SchemaQualified(schema));
        }

        // Detect context from tokens (ignores strings/comments)
        let base_context = self.detect_context_from_tokens(tokens, cursor_pos);

        // If in FROM clause and CTEs are defined, suggest CTE names too
        if base_context == CompletionContext::Table && !sql_context.ctes.is_empty() {
            return (current_token, CompletionContext::CteOrTable);
        }

        (current_token, base_context)
    }

    fn detect_alias_prefix(
        &self,
        before_cursor: &str,
        current_token: &str,
        sql_context: &SqlContext,
    ) -> Option<String> {
        let prefix_end = before_cursor.len().saturating_sub(current_token.len());
        let prefix = &before_cursor[..prefix_end];

        if prefix.ends_with('.') {
            let potential_alias: String = prefix
                .trim_end_matches('.')
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect();

            if !potential_alias.is_empty() {
                // Check if it matches any table alias in the context
                let alias_lower = potential_alias.to_lowercase();
                for table_ref in &sql_context.tables {
                    if let Some(ref alias) = table_ref.alias
                        && alias.to_lowercase() == alias_lower
                    {
                        return Some(potential_alias);
                    }
                    // Also check if it matches the table name directly
                    if table_ref.table.to_lowercase() == alias_lower {
                        return Some(potential_alias);
                    }
                }
            }
        }
        None
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

    fn detect_context_from_tokens(&self, tokens: &[Token], cursor_pos: usize) -> CompletionContext {
        let keywords_table = ["FROM", "JOIN", "INTO", "UPDATE"];
        let keywords_column = ["SELECT", "WHERE", "ON", "SET", "AND", "OR", "BY"];

        let mut last_table_pos = None;
        let mut last_column_pos = None;

        // Only look at tokens before cursor position
        for token in tokens {
            if token.start >= cursor_pos {
                break;
            }

            if let TokenKind::Keyword(kw) = &token.kind {
                let kw_upper = kw.to_uppercase();
                if keywords_table.contains(&kw_upper.as_str()) {
                    last_table_pos = Some(token.start);
                } else if keywords_column.contains(&kw_upper.as_str()) {
                    last_column_pos = Some(token.start);
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

        candidates.into_iter().take(COMPLETION_MAX_CANDIDATES).collect()
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

        candidates.into_iter().take(COMPLETION_MAX_CANDIDATES).collect()
    }

    fn column_candidates(
        &self,
        table_detail: Option<&Table>,
        prefix: &str,
    ) -> Vec<CompletionCandidate> {
        self.column_candidates_with_fk(table_detail, prefix, &[])
    }

    fn column_candidates_with_fk(
        &self,
        table_detail: Option<&Table>,
        prefix: &str,
        recent_columns: &[String],
    ) -> Vec<CompletionCandidate> {
        let Some(table) = table_detail else {
            return vec![];
        };

        let prefix_lower = prefix.to_lowercase();
        let fk_columns: Vec<&str> = table
            .foreign_keys
            .iter()
            .flat_map(|fk| fk.from_columns.iter().map(|s| s.as_str()))
            .collect();

        let mut candidates: Vec<_> = table
            .columns
            .iter()
            .filter(|c| {
                if prefix.is_empty() {
                    return true;
                }
                let name_lower = c.name.to_lowercase();
                name_lower.starts_with(&prefix_lower) || name_lower.contains(&prefix_lower)
            })
            .map(|c| {
                let name_lower = c.name.to_lowercase();
                let is_prefix_match = name_lower.starts_with(&prefix_lower);
                let is_contains_match = !is_prefix_match && name_lower.contains(&prefix_lower);

                let mut score = if is_prefix_match {
                    100
                } else if is_contains_match {
                    10
                } else {
                    0
                };

                // Boost PK columns (+50)
                if c.is_primary_key {
                    score += 50;
                }
                // Boost FK columns (+40)
                if fk_columns.contains(&c.name.as_str()) {
                    score += 40;
                }
                // Boost NOT NULL columns (+20)
                if !c.nullable {
                    score += 20;
                }
                // Boost recently used columns (+30)
                if recent_columns.contains(&c.name) {
                    score += 30;
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
        candidates.sort_by(|a, b| match b.score.cmp(&a.score) {
            std::cmp::Ordering::Equal => a.text.cmp(&b.text),
            other => other,
        });

        candidates.into_iter().take(COMPLETION_MAX_CANDIDATES).collect()
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

        candidates.into_iter().take(COMPLETION_MAX_CANDIDATES).collect()
    }

    fn alias_column_candidates(
        &self,
        alias: &str,
        sql_context: &SqlContext,
        metadata: Option<&DatabaseMetadata>,
        prefix: &str,
    ) -> Vec<CompletionCandidate> {
        let alias_lower = alias.to_lowercase();

        // Find the table reference matching this alias
        let table_ref = sql_context.tables.iter().find(|t| {
            t.alias
                .as_ref()
                .map(|a| a.to_lowercase() == alias_lower)
                .unwrap_or(false)
                || t.table.to_lowercase() == alias_lower
        });

        let Some(table_ref) = table_ref else {
            return vec![];
        };

        // Try to find the table in cache
        let qualified_name = self.qualified_name_from_ref(table_ref, metadata);

        if let Some(table) = self.table_detail_cache.get(&qualified_name) {
            return self.column_candidates(Some(table), prefix);
        }

        // If not in cache, return empty (caller should request table details)
        vec![]
    }

    fn cte_or_table_candidates(
        &self,
        sql_context: &SqlContext,
        metadata: Option<&DatabaseMetadata>,
        prefix: &str,
    ) -> Vec<CompletionCandidate> {
        let prefix_lower = prefix.to_lowercase();
        let mut candidates = Vec::new();

        // Add CTE names first (higher priority)
        for cte in &sql_context.ctes {
            if prefix.is_empty() || cte.name.to_lowercase().starts_with(&prefix_lower) {
                candidates.push(CompletionCandidate {
                    text: cte.name.clone(),
                    kind: CompletionKind::Table,
                    detail: Some("CTE".to_string()),
                    score: 110, // CTEs slightly above prefix-matched tables
                });
            }
        }

        // Add regular tables
        if let Some(metadata) = metadata {
            for t in &metadata.tables {
                if prefix.is_empty()
                    || t.name.to_lowercase().starts_with(&prefix_lower)
                    || t.qualified_name().to_lowercase().starts_with(&prefix_lower)
                {
                    let is_name_prefix = t.name.to_lowercase().starts_with(&prefix_lower);
                    candidates.push(CompletionCandidate {
                        text: t.qualified_name(),
                        kind: CompletionKind::Table,
                        detail: t.row_count_estimate.map(|c| format!("~{} rows", c)),
                        score: if is_name_prefix { 100 } else { 50 },
                    });
                }
            }
        }

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| match b.score.cmp(&a.score) {
            std::cmp::Ordering::Equal => a.text.cmp(&b.text),
            other => other,
        });

        candidates.into_iter().take(COMPLETION_MAX_CANDIDATES).collect()
    }

    fn qualified_name_from_ref(
        &self,
        table_ref: &TableReference,
        metadata: Option<&DatabaseMetadata>,
    ) -> String {
        if let Some(ref schema) = table_ref.schema {
            format!("{}.{}", schema, table_ref.table)
        } else if let Some(metadata) = metadata {
            // Try to find the table and get its schema
            metadata
                .tables
                .iter()
                .find(|t| t.name.to_lowercase() == table_ref.table.to_lowercase())
                .map(|t| t.qualified_name())
                .unwrap_or_else(|| table_ref.table.clone())
        } else {
            table_ref.table.clone()
        }
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
        fn schema_qualified_candidates_limited_to_max() {
            let e = engine();

            // Create metadata with 25 tables in the same schema
            let mut tables = vec![];
            for i in 0..25 {
                tables.push(TableSummary::new(
                    "public".to_string(),
                    format!("table_{:02}", i),
                    Some(100),
                    false,
                ));
            }

            let mut metadata = DatabaseMetadata::new("test_db".to_string());
            metadata.tables = tables;

            let candidates =
                e.schema_qualified_candidates(Some(&metadata), "public", "table");

            // Should be limited to COMPLETION_MAX_CANDIDATES
            assert_eq!(candidates.len(), COMPLETION_MAX_CANDIDATES);
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

    mod alias_column_context {
        use super::*;
        use crate::app::sql_lexer::{SqlContext, TableReference};

        #[test]
        fn alias_dot_returns_alias_column_context() {
            let e = engine();
            let sql = "SELECT u.";
            let tokens = e.lexer.tokenize(sql, sql.len(), None);
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                current_clause: Default::default(),
            };

            let (token, ctx) = e.analyze_with_context(sql, 9, &sql_context, &tokens);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::AliasColumn("u".to_string()));
        }

        #[test]
        fn alias_dot_partial_column_returns_alias_column_context() {
            let e = engine();
            let sql = "SELECT u.na";
            let tokens = e.lexer.tokenize(sql, sql.len(), None);
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                current_clause: Default::default(),
            };

            let (token, ctx) = e.analyze_with_context(sql, 11, &sql_context, &tokens);

            assert_eq!(token, "na");
            assert_eq!(ctx, CompletionContext::AliasColumn("u".to_string()));
        }

        #[test]
        fn table_name_dot_returns_alias_column_context() {
            let e = engine();
            let sql = "SELECT users.";
            let tokens = e.lexer.tokenize(sql, sql.len(), None);
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: None,
                    position: 0,
                }],
                ctes: vec![],
                current_clause: Default::default(),
            };

            let (token, ctx) = e.analyze_with_context(sql, 13, &sql_context, &tokens);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::AliasColumn("users".to_string()));
        }

        #[test]
        fn unknown_alias_dot_returns_schema_qualified() {
            let e = engine();
            let sql = "SELECT public.";
            let tokens = e.lexer.tokenize(sql, sql.len(), None);
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                current_clause: Default::default(),
            };

            let (token, ctx) = e.analyze_with_context(sql, 14, &sql_context, &tokens);

            // "public" is not a known alias, so it falls back to schema-qualified
            assert_eq!(token, "");
            assert_eq!(
                ctx,
                CompletionContext::SchemaQualified("public".to_string())
            );
        }
    }

    mod cte_or_table_context {
        use super::*;
        use crate::app::sql_lexer::{CteDefinition, SqlContext};
        use crate::domain::DatabaseMetadata;

        #[test]
        fn from_clause_with_cte_returns_cte_or_table() {
            let e = engine();
            let sql = "WITH active_users AS (SELECT 1) SELECT * FROM ";
            let tokens = e.lexer.tokenize(sql, sql.len(), None);
            let sql_context = SqlContext {
                tables: vec![],
                ctes: vec![CteDefinition {
                    name: "active_users".to_string(),
                    position: 5,
                }],
                current_clause: Default::default(),
            };

            let (token, ctx) = e.analyze_with_context(sql, 46, &sql_context, &tokens);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::CteOrTable);
        }

        #[test]
        fn cte_candidates_ranked_higher_than_tables() {
            let e = engine();
            let sql_context = SqlContext {
                tables: vec![],
                ctes: vec![CteDefinition {
                    name: "active_users".to_string(),
                    position: 5,
                }],
                current_clause: Default::default(),
            };

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![crate::domain::TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            let candidates = e.cte_or_table_candidates(&sql_context, Some(&metadata), "");

            // CTE should come first with highest score
            assert!(!candidates.is_empty());
            assert_eq!(candidates[0].text, "active_users");
            assert_eq!(candidates[0].detail, Some("CTE".to_string()));
            assert!(candidates[0].score > candidates[1].score);
        }

        #[test]
        fn cte_prefix_filter_works() {
            let e = engine();
            let sql_context = SqlContext {
                tables: vec![],
                ctes: vec![
                    CteDefinition {
                        name: "active_users".to_string(),
                        position: 5,
                    },
                    CteDefinition {
                        name: "banned_users".to_string(),
                        position: 50,
                    },
                ],
                current_clause: Default::default(),
            };

            let candidates = e.cte_or_table_candidates(&sql_context, None, "act");

            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].text, "active_users");
        }
    }

    mod alias_column_completion {
        use super::*;
        use crate::app::sql_lexer::{SqlContext, TableReference};
        use crate::domain::{Column, DatabaseMetadata, Table, TableSummary};

        #[test]
        fn cached_table_returns_columns() {
            let mut e = engine();

            let table = Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        data_type: "int".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: true,
                        is_unique: true,
                        comment: None,
                        ordinal_position: 1,
                    },
                    Column {
                        name: "name".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
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

            e.cache_table_detail("public.users".to_string(), table);

            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: Some("public".to_string()),
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                current_clause: Default::default(),
            };

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            let candidates = e.alias_column_candidates("u", &sql_context, Some(&metadata), "");

            assert_eq!(candidates.len(), 2);
            assert!(candidates.iter().any(|c| c.text == "id"));
            assert!(candidates.iter().any(|c| c.text == "name"));
        }

        #[test]
        fn non_cached_table_returns_empty() {
            let e = engine();

            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                current_clause: Default::default(),
            };

            let candidates = e.alias_column_candidates("u", &sql_context, None, "");

            assert!(candidates.is_empty());
        }

        #[test]
        fn alias_prefix_filters_columns() {
            let mut e = engine();

            let table = Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                columns: vec![
                    Column {
                        name: "user_id".to_string(),
                        data_type: "int".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: true,
                        is_unique: true,
                        comment: None,
                        ordinal_position: 1,
                    },
                    Column {
                        name: "username".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 2,
                    },
                    Column {
                        name: "email".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 3,
                    },
                ],
                primary_key: Some(vec!["user_id".to_string()]),
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                row_count_estimate: None,
                comment: None,
            };

            e.cache_table_detail("public.users".to_string(), table);

            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: Some("public".to_string()),
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                current_clause: Default::default(),
            };

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            let candidates = e.alias_column_candidates("u", &sql_context, Some(&metadata), "user");

            assert_eq!(candidates.len(), 2);
            assert!(candidates.iter().any(|c| c.text == "user_id"));
            assert!(candidates.iter().any(|c| c.text == "username"));
        }
    }

    mod fk_column_scoring {
        use super::*;
        use crate::domain::{Column, ForeignKey, FkAction, Table};

        fn create_table_with_fk() -> Table {
            Table {
                schema: "public".to_string(),
                name: "orders".to_string(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        data_type: "int".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: true,
                        is_unique: true,
                        comment: None,
                        ordinal_position: 1,
                    },
                    Column {
                        name: "user_id".to_string(),
                        data_type: "int".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 2,
                    },
                    Column {
                        name: "status".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 3,
                    },
                ],
                primary_key: Some(vec!["id".to_string()]),
                foreign_keys: vec![ForeignKey {
                    name: "fk_orders_users".to_string(),
                    from_schema: "public".to_string(),
                    from_table: "orders".to_string(),
                    from_columns: vec!["user_id".to_string()],
                    to_schema: "public".to_string(),
                    to_table: "users".to_string(),
                    to_columns: vec!["id".to_string()],
                    on_delete: FkAction::NoAction,
                    on_update: FkAction::NoAction,
                }],
                indexes: vec![],
                rls: None,
                row_count_estimate: None,
                comment: None,
            }
        }

        #[test]
        fn fk_column_returns_higher_score() {
            let e = engine();
            let table = create_table_with_fk();

            let candidates = e.column_candidates_with_fk(Some(&table), "", &[]);

            // id: PK(+50) + NOT NULL(+20) = 170
            // user_id: FK(+40) + NOT NULL(+20) = 160
            // status: nullable = 100
            let id_score = candidates.iter().find(|c| c.text == "id").unwrap().score;
            let user_id_score = candidates.iter().find(|c| c.text == "user_id").unwrap().score;
            let status_score = candidates.iter().find(|c| c.text == "status").unwrap().score;

            assert!(id_score > user_id_score);
            assert!(user_id_score > status_score);
        }

        #[test]
        fn fk_column_with_prefix_match_returns_boosted_score() {
            let e = engine();
            let table = create_table_with_fk();

            let candidates = e.column_candidates_with_fk(Some(&table), "user", &[]);

            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].text, "user_id");
            // Prefix(+100) + FK(+40) + NOT NULL(+20) = 160
            assert_eq!(candidates[0].score, 160);
        }
    }

    mod contains_match {
        use super::*;
        use crate::domain::{Column, Table};

        #[test]
        fn contains_match_returns_candidates() {
            let e = engine();
            let table = Table {
                schema: "public".to_string(),
                name: "test".to_string(),
                columns: vec![
                    Column {
                        name: "user_id".to_string(),
                        data_type: "int".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: 1,
                    },
                    Column {
                        name: "created_at".to_string(),
                        data_type: "timestamp".to_string(),
                        nullable: true,
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

            // "id" is contained in "user_id"
            let candidates = e.column_candidates_with_fk(Some(&table), "id", &[]);

            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].text, "user_id");
        }

        #[test]
        fn prefix_match_ranked_higher_than_contains() {
            let e = engine();
            let table = Table {
                schema: "public".to_string(),
                name: "test".to_string(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        data_type: "int".to_string(),
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
                        nullable: true,
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

            let candidates = e.column_candidates_with_fk(Some(&table), "id", &[]);

            // "id" is prefix match (+100), "user_id" is contains match (+10)
            assert_eq!(candidates.len(), 2);
            assert_eq!(candidates[0].text, "id");
            assert_eq!(candidates[1].text, "user_id");
            assert!(candidates[0].score > candidates[1].score);
        }
    }

    mod recent_columns_scoring {
        use super::*;
        use crate::domain::{Column, Table};

        #[test]
        fn recent_column_returns_boosted_score() {
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
                        name: "email".to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
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

            let recent = vec!["email".to_string()];
            let candidates = e.column_candidates_with_fk(Some(&table), "", &recent);

            // "email" has recent bonus (+30)
            let email_score = candidates.iter().find(|c| c.text == "email").unwrap().score;
            let name_score = candidates.iter().find(|c| c.text == "name").unwrap().score;

            assert!(email_score > name_score);
            assert_eq!(email_score - name_score, 30);
        }
    }

    mod regression_tests {
        use super::*;

        #[test]
        fn select_xxx_f_returns_from_keyword() {
            let e = engine();

            // Column context but no table_detail -> should fallback to keywords
            let candidates = e.get_candidates("SELECT xxx F", 12, None, None);

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.text == "FROM"));
        }

        #[test]
        fn keyword_in_string_does_not_affect_context() {
            let e = engine();

            // "FROM" inside string should not trigger Table context
            let candidates = e.get_candidates("SELECT 'FROM' ", 14, None, None);

            // Should be Column context (after SELECT), but fallback to Keyword
            assert!(!candidates.is_empty());
            // Should not show table candidates (which would be empty anyway)
            assert!(candidates.iter().any(|c| c.kind == CompletionKind::Keyword));
        }

        #[test]
        fn keyword_in_comment_does_not_affect_context() {
            let e = engine();

            // "FROM" inside comment should not trigger Table context
            let candidates = e.get_candidates("SELECT -- FROM\n", 15, None, None);

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.kind == CompletionKind::Keyword));
        }
    }
}
