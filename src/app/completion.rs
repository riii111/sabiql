use crate::app::cache::BoundedLruCache;
use crate::app::sql_lexer::{SqlContext, SqlLexer, TableReference, Token, TokenKind};
use crate::app::sql_modal_context::{CompletionCandidate, CompletionKind};
use crate::domain::{DatabaseMetadata, Table};

const COMPLETION_MAX_CANDIDATES: usize = 30;
const TABLE_CACHE_CAPACITY: usize = 500;

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
    table_detail_cache: BoundedLruCache<String, Table>,
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
            table_detail_cache: BoundedLruCache::new(TABLE_CACHE_CAPACITY),
        }
    }

    #[cfg(test)]
    pub fn new_with_capacity(capacity: usize) -> Self {
        let mut engine = Self::new();
        engine.table_detail_cache = BoundedLruCache::new(capacity);
        engine
    }

    pub fn cache_table_detail(&mut self, qualified_name: String, table: Table) {
        self.table_detail_cache.insert(qualified_name, table);
    }

    pub fn has_cached_table(&self, qualified_name: &str) -> bool {
        self.table_detail_cache.contains(qualified_name)
    }

    pub fn clear_table_cache(&mut self) {
        self.table_detail_cache.clear();
    }

    /// Returns an iterator over cached table details for graph building
    pub fn table_details_iter(&self) -> impl Iterator<Item = (&String, &Table)> {
        self.table_detail_cache.iter()
    }

    /// Returns qualified table names referenced in SQL but not cached (max 10)
    pub fn missing_tables(
        &self,
        content: &str,
        metadata: Option<&DatabaseMetadata>,
    ) -> Vec<String> {
        const MAX_MISSING_TABLES: usize = 10;

        let tokens = self.lexer.tokenize(content, content.len());
        let sql_context = self.lexer.build_context(&tokens, content.len());

        let cte_names: std::collections::HashSet<String> = sql_context
            .ctes
            .iter()
            .map(|cte| cte.name.to_lowercase())
            .collect();

        let mut missing = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for table_ref in &sql_context.tables {
            if cte_names.contains(&table_ref.table.to_lowercase()) {
                continue;
            }

            let qualified_name = self.qualified_name_from_ref(table_ref, metadata);

            if seen.contains(&qualified_name) || self.table_detail_cache.contains(&qualified_name) {
                continue;
            }

            seen.insert(qualified_name.clone());
            missing.push(qualified_name);

            if missing.len() >= MAX_MISSING_TABLES {
                break;
            }
        }

        missing
    }

    pub fn get_candidates(
        &self,
        content: &str,
        cursor_pos: usize,
        metadata: Option<&DatabaseMetadata>,
        table_detail: Option<&Table>,
        recent_columns: &[String],
    ) -> Vec<CompletionCandidate> {
        // Skip completion inside strings or comments
        if self.lexer.is_in_string_or_comment(content, cursor_pos) {
            return vec![];
        }

        // Build SQL context for alias resolution
        let tokens = self.lexer.tokenize(content, content.len());
        let sql_context = self.lexer.build_context(&tokens, cursor_pos);

        let (current_token, context) =
            self.analyze_with_context(content, cursor_pos, &sql_context, &tokens);

        let mut candidates = match &context {
            CompletionContext::Keyword => self.keyword_candidates(&current_token),
            CompletionContext::Table => self.table_candidates(metadata, &current_token),
            CompletionContext::Column => {
                let keywords = self.primary_clause_keywords(&current_token);

                // Check if cursor is right after a comma (column list continuation)
                let before_cursor: String = content.chars().take(cursor_pos).collect();
                let before_token = before_cursor
                    .trim_end()
                    .strip_suffix(&current_token)
                    .unwrap_or(&before_cursor)
                    .trim_end();
                let after_comma = before_token.ends_with(',');

                let target_qualified = sql_context
                    .target_table
                    .as_ref()
                    .map(|t| self.qualified_name_from_ref(t, metadata));

                let mut columns =
                    self.column_candidates_with_fk(table_detail, &current_token, recent_columns);

                // UPDATE/DELETE/INSERT target table columns get priority
                if let (Some(detail), Some(target)) = (table_detail, &target_qualified)
                    && detail.qualified_name() == *target
                {
                    for col in &mut columns {
                        col.score += 200;
                    }
                }

                // Build set of tables referenced in current SQL (excluding CTEs)
                let cte_names: std::collections::HashSet<String> = sql_context
                    .ctes
                    .iter()
                    .map(|cte| cte.name.to_lowercase())
                    .collect();
                let referenced_tables: std::collections::HashSet<String> = sql_context
                    .tables
                    .iter()
                    .filter(|t| !cte_names.contains(&t.table.to_lowercase()))
                    .map(|t| self.qualified_name_from_ref(t, metadata))
                    .collect();

                let selected_qualified = table_detail.map(|t| t.qualified_name());
                let use_all_cache = referenced_tables.is_empty();
                for (qualified_name, cached_table) in self.table_detail_cache.iter() {
                    if selected_qualified.as_ref() == Some(qualified_name) {
                        continue;
                    }
                    if !use_all_cache && !referenced_tables.contains(qualified_name) {
                        continue;
                    }
                    let mut cached_columns = self.column_candidates_with_fk(
                        Some(cached_table),
                        &current_token,
                        recent_columns,
                    );

                    if target_qualified.as_ref() == Some(qualified_name) {
                        for col in &mut cached_columns {
                            col.score += 200;
                        }
                    }
                    columns.extend(cached_columns);
                }

                let has_prefix = current_token.len() >= 2;
                if has_prefix && !columns.is_empty() {
                    for col in &mut columns {
                        if col.score >= 100 {
                            col.score += 250;
                        }
                    }
                }

                // After comma, strongly prefer columns over keywords
                if after_comma {
                    for col in &mut columns {
                        col.score += 300;
                    }
                }

                let max_keywords = if after_comma {
                    3
                } else if has_prefix {
                    5
                } else {
                    15
                }
                .min(keywords.len());
                let max_columns = (COMPLETION_MAX_CANDIDATES - max_keywords).min(columns.len());

                let mut mixed: Vec<_> = keywords.into_iter().take(max_keywords).collect();
                mixed.extend(columns.into_iter().take(max_columns));
                mixed.sort_by(|a, b| match b.score.cmp(&a.score) {
                    std::cmp::Ordering::Equal => a.text.cmp(&b.text),
                    other => other,
                });
                mixed
            }
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

        // Deduplicate by text (keep highest score - first occurrence after sort)
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| seen.insert(c.text.to_uppercase()));

        candidates
    }

    pub fn current_token_len(&self, content: &str, cursor_pos: usize) -> usize {
        let before_cursor: String = content.chars().take(cursor_pos).collect();
        self.extract_current_token(&before_cursor).chars().count()
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
        let mut candidates: Vec<_> = self
            .keywords
            .iter()
            .filter(|kw| prefix.is_empty() || kw.starts_with(&prefix_upper))
            .map(|kw| {
                let is_prefix_match = kw.starts_with(&prefix_upper);
                CompletionCandidate {
                    text: (*kw).to_string(),
                    kind: CompletionKind::Keyword,
                    score: if is_prefix_match { 100 } else { 10 },
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| match b.score.cmp(&a.score) {
            std::cmp::Ordering::Equal => a.text.cmp(&b.text),
            other => other,
        });

        candidates
            .into_iter()
            .take(COMPLETION_MAX_CANDIDATES)
            .collect()
    }

    /// Primary SQL clause keywords that should always appear in Column context
    /// These get high score (200) to ensure they appear above column candidates
    fn primary_clause_keywords(&self, prefix: &str) -> Vec<CompletionCandidate> {
        const PRIMARY_KEYWORDS: &[&str] = &[
            "FROM",
            "WHERE",
            "ORDER",
            "BY", // For ORDER BY, GROUP BY
            "GROUP",
            "HAVING",
            "LIMIT",
            "OFFSET",
            "JOIN",
            "LEFT",
            "RIGHT",
            "INNER",
            "OUTER",
            "CROSS",
            "ON",
            "AND",
            "OR",
            "AS",
            "DISTINCT",
            "UNION",
            "EXCEPT",
            "INTERSECT",
            "CASE",
            "WHEN",
            "THEN",
            "ELSE",
            "END",
            "IN",
            "NOT",
            "NULL",
            "LIKE",
            "BETWEEN",
            "EXISTS",
            "IS",
        ];

        let prefix_upper = prefix.to_uppercase();
        PRIMARY_KEYWORDS
            .iter()
            .filter(|kw| prefix.is_empty() || kw.starts_with(&prefix_upper))
            .map(|kw| CompletionCandidate {
                text: (*kw).to_string(),
                kind: CompletionKind::Keyword,
                score: 200, // Higher than column scores (max ~170)
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
                let is_qualified_prefix =
                    t.qualified_name().to_lowercase().starts_with(&prefix_lower);
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
                    score,
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| match b.score.cmp(&a.score) {
            std::cmp::Ordering::Equal => a.text.cmp(&b.text),
            other => other,
        });

        candidates
            .into_iter()
            .take(COMPLETION_MAX_CANDIDATES)
            .collect()
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
                    score,
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| match b.score.cmp(&a.score) {
            std::cmp::Ordering::Equal => a.text.cmp(&b.text),
            other => other,
        });

        candidates
            .into_iter()
            .take(COMPLETION_MAX_CANDIDATES)
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
                    score: if is_prefix_match { 100 } else { 10 },
                }
            })
            .collect();

        // Sort by score (descending), then alphabetically
        candidates.sort_by(|a, b| match b.score.cmp(&a.score) {
            std::cmp::Ordering::Equal => a.text.cmp(&b.text),
            other => other,
        });

        candidates
            .into_iter()
            .take(COMPLETION_MAX_CANDIDATES)
            .collect()
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

        if let Some(table) = self.table_detail_cache.peek(&qualified_name) {
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

        candidates
            .into_iter()
            .take(COMPLETION_MAX_CANDIDATES)
            .collect()
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
impl CompletionEngine {
    fn analyze(&self, content: &str, cursor_pos: usize) -> (String, CompletionContext) {
        let tokens = self.lexer.tokenize(content, cursor_pos);
        let sql_context = SqlContext::default();
        self.analyze_with_context(content, cursor_pos, &sql_context, &tokens)
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

            // Create metadata with 35 tables in the same schema (more than COMPLETION_MAX_CANDIDATES)
            let mut tables = vec![];
            for i in 0..35 {
                tables.push(TableSummary::new(
                    "public".to_string(),
                    format!("table_{:02}", i),
                    Some(100),
                    false,
                ));
            }

            let mut metadata = DatabaseMetadata::new("test_db".to_string());
            metadata.tables = tables;

            let candidates = e.schema_qualified_candidates(Some(&metadata), "public", "table");

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

            let candidates = e.schema_qualified_candidates(Some(&metadata), "myschema", "");

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
                owner: None,
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
                triggers: vec![],
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

            let candidates = e.get_candidates("SELECT 'SEL", 11, None, None, &[]);

            assert!(candidates.is_empty());
        }

        #[test]
        fn inside_line_comment_returns_empty() {
            let e = engine();

            let candidates = e.get_candidates("-- SEL", 6, None, None, &[]);

            assert!(candidates.is_empty());
        }

        #[test]
        fn inside_block_comment_returns_empty() {
            let e = engine();

            let candidates = e.get_candidates("/* SEL", 6, None, None, &[]);

            assert!(candidates.is_empty());
        }

        #[test]
        fn inside_dollar_quote_returns_empty() {
            let e = engine();

            let candidates = e.get_candidates("SELECT $$SEL", 12, None, None, &[]);

            assert!(candidates.is_empty());
        }

        #[test]
        fn after_closed_string_returns_candidates() {
            let e = engine();

            let candidates = e.get_candidates("'value' SEL", 11, None, None, &[]);

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.text == "SELECT"));
        }

        #[test]
        fn after_closed_comment_returns_candidates() {
            let e = engine();

            let candidates = e.get_candidates("/* comment */ SEL", 17, None, None, &[]);

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
                owner: None,
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
                triggers: vec![],
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
                owner: None,
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
                triggers: vec![],
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
            let tokens = e.lexer.tokenize(sql, sql.len());
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                target_table: None,
            };

            let (token, ctx) = e.analyze_with_context(sql, 9, &sql_context, &tokens);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::AliasColumn("u".to_string()));
        }

        #[test]
        fn alias_dot_partial_column_returns_alias_column_context() {
            let e = engine();
            let sql = "SELECT u.na";
            let tokens = e.lexer.tokenize(sql, sql.len());
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                target_table: None,
            };

            let (token, ctx) = e.analyze_with_context(sql, 11, &sql_context, &tokens);

            assert_eq!(token, "na");
            assert_eq!(ctx, CompletionContext::AliasColumn("u".to_string()));
        }

        #[test]
        fn table_name_dot_returns_alias_column_context() {
            let e = engine();
            let sql = "SELECT users.";
            let tokens = e.lexer.tokenize(sql, sql.len());
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: None,
                    position: 0,
                }],
                ctes: vec![],
                target_table: None,
            };

            let (token, ctx) = e.analyze_with_context(sql, 13, &sql_context, &tokens);

            assert_eq!(token, "");
            assert_eq!(ctx, CompletionContext::AliasColumn("users".to_string()));
        }

        #[test]
        fn unknown_alias_dot_returns_schema_qualified() {
            let e = engine();
            let sql = "SELECT public.";
            let tokens = e.lexer.tokenize(sql, sql.len());
            let sql_context = SqlContext {
                tables: vec![TableReference {
                    schema: None,
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                    position: 0,
                }],
                ctes: vec![],
                target_table: None,
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
            let tokens = e.lexer.tokenize(sql, sql.len());
            let sql_context = SqlContext {
                tables: vec![],
                ctes: vec![CteDefinition {
                    name: "active_users".to_string(),
                    position: 5,
                }],
                target_table: None,
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
                target_table: None,
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
                target_table: None,
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
                owner: None,
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
                triggers: vec![],
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
                target_table: None,
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
                target_table: None,
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
                owner: None,
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
                triggers: vec![],
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
                target_table: None,
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
        use crate::domain::{Column, FkAction, ForeignKey, Table};

        fn create_table_with_fk() -> Table {
            Table {
                schema: "public".to_string(),
                name: "orders".to_string(),
                owner: None,
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
                triggers: vec![],
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
            let user_id_score = candidates
                .iter()
                .find(|c| c.text == "user_id")
                .unwrap()
                .score;
            let status_score = candidates
                .iter()
                .find(|c| c.text == "status")
                .unwrap()
                .score;

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
                owner: None,
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
                triggers: vec![],
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
                owner: None,
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
                triggers: vec![],
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
                owner: None,
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
                triggers: vec![],
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
            let candidates = e.get_candidates("SELECT xxx F", 12, None, None, &[]);

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.text == "FROM"));
        }

        #[test]
        fn keyword_in_string_does_not_affect_context() {
            let e = engine();

            // "FROM" inside string should not trigger Table context
            let candidates = e.get_candidates("SELECT 'FROM' ", 14, None, None, &[]);

            // Should be Column context (after SELECT), but fallback to Keyword
            assert!(!candidates.is_empty());
            // Should not show table candidates (which would be empty anyway)
            assert!(candidates.iter().any(|c| c.kind == CompletionKind::Keyword));
        }

        #[test]
        fn keyword_in_comment_does_not_affect_context() {
            let e = engine();

            // "FROM" inside comment should not trigger Table context
            let candidates = e.get_candidates("SELECT -- FROM\n", 15, None, None, &[]);

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.kind == CompletionKind::Keyword));
        }
    }

    mod missing_tables {
        use super::*;
        use crate::domain::{Column, DatabaseMetadata, Table, TableSummary};

        #[test]
        fn empty_sql_returns_empty() {
            let e = engine();

            let missing = e.missing_tables("", None);

            assert!(missing.is_empty());
        }

        #[test]
        fn simple_from_returns_table() {
            let e = engine();
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            let missing = e.missing_tables("SELECT * FROM users", Some(&metadata));

            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], "public.users");
        }

        #[test]
        fn schema_qualified_table_returns_qualified_name() {
            let e = engine();

            let missing = e.missing_tables("SELECT * FROM public.orders", None);

            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], "public.orders");
        }

        #[test]
        fn multiple_tables_returns_all() {
            let e = engine();
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![
                TableSummary::new("public".to_string(), "users".to_string(), None, false),
                TableSummary::new("public".to_string(), "orders".to_string(), None, false),
            ];

            let missing = e.missing_tables(
                "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
                Some(&metadata),
            );

            assert_eq!(missing.len(), 2);
            assert!(missing.contains(&"public.users".to_string()));
            assert!(missing.contains(&"public.orders".to_string()));
        }

        #[test]
        fn cached_tables_are_excluded() {
            let mut e = engine();
            let table = Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![Column {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                }],
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            };
            e.cache_table_detail("public.users".to_string(), table);

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![
                TableSummary::new("public".to_string(), "users".to_string(), None, false),
                TableSummary::new("public".to_string(), "orders".to_string(), None, false),
            ];

            let missing = e.missing_tables(
                "SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
                Some(&metadata),
            );

            // users is cached, so only orders should be missing
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], "public.orders");
        }

        #[test]
        fn cte_tables_are_excluded() {
            let e = engine();
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            let missing = e.missing_tables(
                "WITH recent AS (SELECT * FROM users) SELECT * FROM recent",
                Some(&metadata),
            );

            // "recent" is CTE, so only "users" should be returned
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], "public.users");
        }

        #[test]
        fn duplicate_tables_are_deduplicated() {
            let e = engine();
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            let missing = e.missing_tables(
                "SELECT * FROM users u1 JOIN users u2 ON u1.id = u2.id",
                Some(&metadata),
            );

            // users appears twice but should be deduplicated
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], "public.users");
        }

        #[test]
        fn max_limit_is_respected() {
            let e = engine();

            // Use schema-qualified tables to avoid metadata lookup issues
            // Build SQL with 15 JOINs to ensure parser recognizes all tables
            let joins = (1..15)
                .map(|i| format!("JOIN public.table_{} t{} ON t0.id = t{}.id", i, i, i))
                .collect::<Vec<_>>()
                .join(" ");
            let sql = format!("SELECT * FROM public.table_0 t0 {}", joins);
            let missing = e.missing_tables(&sql, None);

            // MAX_MISSING_TABLES = 10, so even with 15 tables, only 10 should be returned
            assert_eq!(missing.len(), 10);
        }

        #[test]
        fn has_cached_table_returns_true_for_cached() {
            let mut e = engine();
            let table = Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![],
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            };
            e.cache_table_detail("public.users".to_string(), table);

            assert!(e.has_cached_table("public.users"));
            assert!(!e.has_cached_table("public.orders"));
        }
    }

    mod integration_tests {
        use super::*;
        use crate::domain::{Column, DatabaseMetadata, Table, TableSummary};

        fn create_users_table() -> Table {
            Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
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
                    Column {
                        name: "email".to_string(),
                        data_type: "text".to_string(),
                        nullable: false,
                        default: None,
                        is_primary_key: false,
                        is_unique: true,
                        comment: None,
                        ordinal_position: 3,
                    },
                ],
                primary_key: Some(vec!["id".to_string()]),
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        #[test]
        fn get_candidates_with_table_detail_returns_columns() {
            let e = engine();
            let table = create_users_table();

            // SELECT context with table_detail should return columns
            let candidates = e.get_candidates("SELECT ", 7, None, Some(&table), &[]);

            assert!(!candidates.is_empty());
            assert!(
                candidates
                    .iter()
                    .any(|c| c.text == "id" && c.kind == CompletionKind::Column)
            );
            assert!(
                candidates
                    .iter()
                    .any(|c| c.text == "name" && c.kind == CompletionKind::Column)
            );
            assert!(
                candidates
                    .iter()
                    .any(|c| c.text == "email" && c.kind == CompletionKind::Column)
            );
        }

        #[test]
        fn get_candidates_with_cached_table_returns_alias_columns() {
            let mut e = engine();
            let table = create_users_table();

            // Cache the table
            e.cache_table_detail("public.users".to_string(), table);

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            // "u." should trigger alias column completion from cache
            let candidates = e.get_candidates(
                "SELECT u. FROM public.users u",
                9,
                Some(&metadata),
                None,
                &[],
            );

            assert!(!candidates.is_empty());
            assert!(candidates.iter().any(|c| c.text == "id"));
            assert!(candidates.iter().any(|c| c.text == "name"));
            assert!(candidates.iter().any(|c| c.text == "email"));
        }

        #[test]
        fn select_clause_with_table_detail_shows_column_candidates() {
            let e = engine();
            let table = create_users_table();

            // Typing after SELECT with table_detail should show columns
            let candidates = e.get_candidates("SELECT n", 8, None, Some(&table), &[]);

            // Should include "name" column that starts with "n"
            assert!(
                candidates
                    .iter()
                    .any(|c| c.text == "name" && c.kind == CompletionKind::Column)
            );
        }

        #[test]
        fn where_clause_with_table_detail_shows_column_candidates() {
            let e = engine();
            let table = create_users_table();

            // WHERE context with table_detail should return columns
            let candidates =
                e.get_candidates("SELECT * FROM users WHERE ", 26, None, Some(&table), &[]);

            assert!(!candidates.is_empty());
            assert!(
                candidates
                    .iter()
                    .any(|c| c.text == "id" && c.kind == CompletionKind::Column)
            );
        }

        #[test]
        fn alias_completion_without_cache_falls_back_to_keywords() {
            let e = engine();

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            // "u." without cache should fallback to keywords
            let candidates = e.get_candidates(
                "SELECT u. FROM public.users u",
                9,
                Some(&metadata),
                None,
                &[],
            );

            // Should fallback to keywords since cache is empty
            assert!(candidates.iter().any(|c| c.kind == CompletionKind::Keyword));
        }

        #[test]
        fn from_keyword_appears_even_with_column_candidates() {
            let e = engine();
            let table = create_users_table();

            // "SELECT xxx F" with table_detail - should show both FROM keyword and columns starting with F
            let candidates = e.get_candidates("SELECT xxx F", 12, None, Some(&table), &[]);

            // FROM keyword should appear (high priority)
            assert!(
                candidates.iter().any(|c| c.text == "FROM"),
                "FROM keyword should appear in candidates"
            );

            // Verify FROM has higher score than columns
            let from_candidate = candidates.iter().find(|c| c.text == "FROM").unwrap();
            assert_eq!(from_candidate.score, 200, "FROM should have score 200");
        }

        #[test]
        fn column_context_mixes_keywords_and_columns() {
            let e = engine();
            let table = create_users_table();

            // SELECT context should show both keywords and columns
            let candidates = e.get_candidates("SELECT ", 7, None, Some(&table), &[]);

            // Should have keywords
            assert!(
                candidates.iter().any(|c| c.kind == CompletionKind::Keyword),
                "Should include keywords"
            );

            // Should have columns
            assert!(
                candidates.iter().any(|c| c.kind == CompletionKind::Column),
                "Should include columns"
            );

            // Keywords should be ranked higher
            let first_keyword_idx = candidates
                .iter()
                .position(|c| c.kind == CompletionKind::Keyword);
            let first_column_idx = candidates
                .iter()
                .position(|c| c.kind == CompletionKind::Column);

            assert!(
                first_keyword_idx < first_column_idx,
                "Keywords should appear before columns"
            );
        }

        #[test]
        fn order_by_keywords_appear_together() {
            let e = engine();
            let table = create_users_table();

            // After "ORDER ", BY should appear in candidates
            let candidates =
                e.get_candidates("SELECT * FROM t ORDER ", 22, None, Some(&table), &[]);

            assert!(
                candidates.iter().any(|c| c.text == "BY"),
                "BY keyword should appear after ORDER"
            );
        }

        #[test]
        fn duplicate_text_is_deduplicated() {
            let e = engine();

            // Create a table with a column named "and" (same as keyword)
            let table = Table {
                schema: "public".to_string(),
                name: "test".to_string(),
                owner: None,
                columns: vec![Column {
                    name: "and".to_string(), // Same as keyword AND
                    data_type: "text".to_string(),
                    nullable: true,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: 1,
                }],
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            };

            let candidates = e.get_candidates("SELECT ", 7, None, Some(&table), &[]);

            // Count how many times "AND" appears (case-insensitive)
            let and_count = candidates
                .iter()
                .filter(|c| c.text.to_uppercase() == "AND")
                .count();

            assert_eq!(and_count, 1, "AND should appear only once (deduplicated)");
        }

        #[test]
        fn empty_prefix_shows_keywords_first() {
            let e = engine();
            let table = create_users_table();

            // Empty prefix: keywords should come first
            let candidates = e.get_candidates("SELECT ", 7, None, Some(&table), &[]);

            // First candidate should be a keyword (score 200)
            assert_eq!(
                candidates[0].kind,
                CompletionKind::Keyword,
                "With empty prefix, keywords should come first"
            );
        }

        #[test]
        fn non_empty_prefix_shows_columns_first() {
            let e = engine();
            let table = create_users_table();

            // "na" prefix: "name" column should come before keywords
            let candidates = e.get_candidates("SELECT na", 9, None, Some(&table), &[]);

            // First candidate should be the "name" column (boosted score)
            assert_eq!(candidates[0].text, "name");
            assert_eq!(
                candidates[0].kind,
                CompletionKind::Column,
                "With prefix, matching columns should come first"
            );
        }

        #[test]
        fn single_char_prefix_keeps_keywords_first() {
            let e = engine();
            let table = create_users_table();

            // 1 char prefix: keywords stay first (no boost)
            let candidates = e.get_candidates("SELECT n", 8, None, Some(&table), &[]);

            assert!(candidates.iter().any(|c| c.text == "name"));
            assert!(candidates.iter().any(|c| c.text == "NOT"));
            // Keyword should be first with 1-char prefix
            assert_eq!(candidates[0].kind, CompletionKind::Keyword);
        }

        #[test]
        fn two_char_prefix_boosts_columns() {
            let e = engine();
            let table = create_users_table();

            // 2+ char prefix: columns get boosted
            let candidates = e.get_candidates("SELECT na", 9, None, Some(&table), &[]);

            assert_eq!(candidates[0].text, "name");
            assert_eq!(candidates[0].kind, CompletionKind::Column);
        }
    }

    mod target_table_boost {
        use super::*;
        use crate::domain::{Column, DatabaseMetadata, Table, TableSummary};

        fn create_table(schema: &str, name: &str, columns: &[&str]) -> Table {
            Table {
                schema: schema.to_string(),
                name: name.to_string(),
                owner: None,
                columns: columns
                    .iter()
                    .enumerate()
                    .map(|(i, col)| Column {
                        name: (*col).to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: (i + 1) as i32,
                    })
                    .collect(),
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        #[test]
        fn update_target_columns_get_boost() {
            let mut e = engine();
            let users = create_table("public", "users", &["id", "name", "email"]);
            let orders = create_table("public", "orders", &["id", "user_id", "total"]);

            // Cache both tables
            e.cache_table_detail("public.users".to_string(), users.clone());
            e.cache_table_detail("public.orders".to_string(), orders.clone());

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![
                TableSummary::new("public".to_string(), "users".to_string(), None, false),
                TableSummary::new("public".to_string(), "orders".to_string(), None, false),
            ];

            // UPDATE users with subquery referencing orders
            // Both tables are in SQL, but users is the target
            let candidates = e.get_candidates(
                "UPDATE users SET name = (SELECT user_id FROM orders) WHERE ",
                59,
                Some(&metadata),
                Some(&users),
                &[],
            );

            // Find columns from both tables
            let users_name = candidates.iter().find(|c| c.text == "name");
            let orders_user_id = candidates.iter().find(|c| c.text == "user_id");

            assert!(users_name.is_some(), "users.name should be in candidates");
            assert!(
                orders_user_id.is_some(),
                "orders.user_id should be in candidates"
            );

            // Target table column (users.name) should have higher score than non-target (orders.user_id)
            assert!(
                users_name.unwrap().score > orders_user_id.unwrap().score,
                "Target table column should be prioritized"
            );
        }

        #[test]
        fn select_has_no_target_boost() {
            let mut e = engine();
            let users = create_table("public", "users", &["id", "name"]);

            e.cache_table_detail("public.users".to_string(), users.clone());

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "users".to_string(),
                None,
                false,
            )];

            // SELECT has no target, so no boost
            let candidates = e.get_candidates("SELECT ", 7, Some(&metadata), Some(&users), &[]);

            let name_candidate = candidates.iter().find(|c| c.text == "name");
            assert!(name_candidate.is_some());
            // No target boost, base score only (0 for empty prefix)
            assert!(name_candidate.unwrap().score < 200);
        }
    }

    mod all_cache_columns {
        use super::*;
        use crate::domain::{Column, DatabaseMetadata, Table, TableSummary};

        fn create_table(schema: &str, name: &str, columns: &[&str]) -> Table {
            Table {
                schema: schema.to_string(),
                name: name.to_string(),
                owner: None,
                columns: columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Column {
                        name: c.to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: (i + 1) as i32,
                    })
                    .collect(),
                primary_key: None,
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        #[test]
        fn no_from_with_2char_prefix_returns_all_cached_columns() {
            let mut e = engine();
            let users = create_table("public", "users", &["id", "name", "email"]);
            let orders = create_table("public", "orders", &["id", "user_id", "total"]);
            e.cache_table_detail("public.users".to_string(), users);
            e.cache_table_detail("public.orders".to_string(), orders);
            let metadata = DatabaseMetadata::new("test".to_string());

            let candidates = e.get_candidates("SELECT na", 9, Some(&metadata), None, &[]);

            let name_candidate = candidates.iter().find(|c| c.text == "name");
            assert!(name_candidate.is_some());
        }

        #[test]
        fn no_from_with_empty_prefix_returns_all_cached_columns() {
            let mut e = engine();
            let users = create_table("public", "users", &["id", "name"]);
            let orders = create_table("public", "orders", &["order_id", "user_id"]);
            e.cache_table_detail("public.users".to_string(), users);
            e.cache_table_detail("public.orders".to_string(), orders);
            let metadata = DatabaseMetadata::new("test".to_string());

            let candidates = e.get_candidates("SELECT ", 7, Some(&metadata), None, &[]);

            let column_count = candidates
                .iter()
                .filter(|c| c.kind == CompletionKind::Column)
                .count();
            assert!(column_count > 0);
        }

        #[test]
        fn from_clause_present_returns_only_referenced_table_columns() {
            let mut e = engine();
            let users = create_table("public", "users", &["id", "name"]);
            let orders = create_table("public", "orders", &["order_id", "user_id"]);
            e.cache_table_detail("public.users".to_string(), users);
            e.cache_table_detail("public.orders".to_string(), orders);
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![
                TableSummary::new("public".to_string(), "users".to_string(), None, false),
                TableSummary::new("public".to_string(), "orders".to_string(), None, false),
            ];

            let candidates =
                e.get_candidates("SELECT na FROM users", 9, Some(&metadata), None, &[]);

            let name = candidates.iter().find(|c| c.text == "name");
            let user_id = candidates.iter().find(|c| c.text == "user_id");
            assert!(name.is_some());
            assert!(user_id.is_none());
        }
    }

    mod lru_cache_behavior {
        use super::*;
        use crate::domain::{Column, Table, TableSummary};

        fn create_table(schema: &str, name: &str, columns: &[&str]) -> Table {
            Table {
                schema: schema.to_string(),
                name: name.to_string(),
                owner: None,
                columns: columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Column {
                        name: c.to_string(),
                        data_type: "text".to_string(),
                        nullable: true,
                        default: None,
                        is_primary_key: false,
                        is_unique: false,
                        comment: None,
                        ordinal_position: i as i32,
                    })
                    .collect(),
                primary_key: None,
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        #[test]
        fn evicted_table_appears_in_missing_tables() {
            let mut e = CompletionEngine::new_with_capacity(2);

            // Cache 3 tables with capacity 2 - t1 will be evicted
            let t1 = create_table("public", "t1", &["id"]);
            let t2 = create_table("public", "t2", &["id"]);
            let t3 = create_table("public", "t3", &["id"]);

            e.cache_table_detail("public.t1".to_string(), t1);
            e.cache_table_detail("public.t2".to_string(), t2);
            e.cache_table_detail("public.t3".to_string(), t3);

            // t1 should be evicted
            assert!(!e.has_cached_table("public.t1"));
            assert!(e.has_cached_table("public.t2"));
            assert!(e.has_cached_table("public.t3"));

            // Create metadata with all tables
            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![
                TableSummary::new("public".to_string(), "t1".to_string(), None, false),
                TableSummary::new("public".to_string(), "t2".to_string(), None, false),
                TableSummary::new("public".to_string(), "t3".to_string(), None, false),
            ];

            // SQL referencing evicted table should trigger re-fetch
            let missing = e.missing_tables("SELECT * FROM t1", Some(&metadata));
            assert_eq!(missing, vec!["public.t1".to_string()]);
        }

        #[test]
        fn cached_table_not_in_missing_tables() {
            let mut e = CompletionEngine::new_with_capacity(2);

            let t1 = create_table("public", "t1", &["id"]);
            e.cache_table_detail("public.t1".to_string(), t1);

            let mut metadata = DatabaseMetadata::new("test".to_string());
            metadata.tables = vec![TableSummary::new(
                "public".to_string(),
                "t1".to_string(),
                None,
                false,
            )];

            let missing = e.missing_tables("SELECT * FROM t1", Some(&metadata));
            assert!(missing.is_empty());
        }

        #[test]
        fn table_details_iter_returns_all_cached() {
            let mut e = CompletionEngine::new_with_capacity(3);

            let t1 = create_table("public", "t1", &["id"]);
            let t2 = create_table("public", "t2", &["id"]);
            e.cache_table_detail("public.t1".to_string(), t1);
            e.cache_table_detail("public.t2".to_string(), t2);

            let names: Vec<_> = e.table_details_iter().map(|(k, _)| k.clone()).collect();
            assert_eq!(names.len(), 2);
            assert!(names.contains(&"public.t1".to_string()));
            assert!(names.contains(&"public.t2".to_string()));
        }

        #[test]
        fn clear_removes_all_cached_tables() {
            let mut e = CompletionEngine::new_with_capacity(3);

            let t1 = create_table("public", "t1", &["id"]);
            let t2 = create_table("public", "t2", &["id"]);
            e.cache_table_detail("public.t1".to_string(), t1);
            e.cache_table_detail("public.t2".to_string(), t2);

            assert!(e.has_cached_table("public.t1"));
            assert!(e.has_cached_table("public.t2"));

            e.clear_table_cache();

            assert!(!e.has_cached_table("public.t1"));
            assert!(!e.has_cached_table("public.t2"));
            assert_eq!(e.table_details_iter().count(), 0);
        }

        #[test]
        fn lru_eviction_order_is_fifo_without_access() {
            let mut e = CompletionEngine::new_with_capacity(2);

            // Insert t1, t2, t3 in order - t1 should be evicted first
            e.cache_table_detail(
                "public.t1".to_string(),
                create_table("public", "t1", &["id"]),
            );
            e.cache_table_detail(
                "public.t2".to_string(),
                create_table("public", "t2", &["id"]),
            );
            // t1 is now LRU, will be evicted when t3 is added
            e.cache_table_detail(
                "public.t3".to_string(),
                create_table("public", "t3", &["id"]),
            );

            assert!(!e.has_cached_table("public.t1")); // evicted
            assert!(e.has_cached_table("public.t2"));
            assert!(e.has_cached_table("public.t3"));
        }
    }
}
