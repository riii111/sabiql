//! Lightweight SQL lexer for completion context detection.
//!
//! Handles PostgreSQL-specific syntax including:
//! - Dollar-quoted strings ($tag$...$tag$)
//! - Escape strings (E'...')
//! - Line comments (--)
//! - Block comments (/* */)
//! - Cast operator (::)
//! - Array access ([])

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(String),
    Identifier(String),
    Operator(String),
    Punctuation(char),
    StringLiteral,
    Number,
    Comment,
    Whitespace,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    #[allow(dead_code)] // Phase 4: used for detailed token analysis
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReference {
    pub schema: Option<String>,
    pub table: String,
    pub alias: Option<String>,
    pub position: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CteDefinition {
    pub name: String,
    pub position: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ClauseKind {
    #[default]
    Unknown,
    Select,
    From,
    Join,
    Where,
    On,
    GroupBy,
    OrderBy,
    Having,
    InsertInto,
    UpdateSet,
    With,
}

#[derive(Debug, Clone, Default)]
pub struct SqlContext {
    pub tables: Vec<TableReference>,
    pub ctes: Vec<CteDefinition>,
    #[allow(dead_code)] // Phase 4: clause-based completion logic
    pub current_clause: ClauseKind,
    /// Target table for UPDATE/DELETE/INSERT statements (for column priority boost)
    pub target_table: Option<TableReference>,
}

#[derive(Debug, Clone, Default)]
pub struct TokenCache {
    pub tokens: Vec<Token>,
    pub last_content_hash: u64,
    pub last_cursor_pos: usize,
}

impl TokenCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn content_hash(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    pub fn is_valid(&self, content: &str, cursor_pos: usize) -> bool {
        let hash = Self::content_hash(content);
        self.last_content_hash == hash && self.last_cursor_pos == cursor_pos
    }

    #[allow(dead_code)] // Phase 3: differential tokenization
    pub fn update(&mut self, content: &str, cursor_pos: usize, tokens: Vec<Token>) {
        self.tokens = tokens;
        self.last_content_hash = Self::content_hash(content);
        self.last_cursor_pos = cursor_pos;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexerState {
    Normal,
    InSingleQuote,
    InDoubleQuote,
    InDollarQuote,
    InLineComment,
    InBlockComment,
    InEscapeString,
}

const SQL_KEYWORDS: &[&str] = &[
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
    "ONLY",
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
    "FULL",
    "NATURAL",
    "LATERAL",
    "WINDOW",
    "OVER",
    "PARTITION",
    "ROWS",
    "RANGE",
    "UNBOUNDED",
    "PRECEDING",
    "FOLLOWING",
    "CURRENT",
    "ROW",
];

pub struct SqlLexer;

impl SqlLexer {
    pub fn new() -> Self {
        Self
    }

    pub fn tokenize(
        &self,
        text: &str,
        cursor_pos: usize,
        cache: Option<&TokenCache>,
    ) -> Vec<Token> {
        // Check cache validity
        if let Some(cache) = cache
            && cache.is_valid(text, cursor_pos)
        {
            return cache.tokens.clone();
        }

        let chars: Vec<char> = text.chars().collect();
        let end_pos = cursor_pos.min(chars.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        let mut state = LexerState::Normal;
        let mut token_start = 0;
        let mut dollar_tag = String::new();

        while pos < end_pos {
            let c = chars[pos];

            match state {
                LexerState::Normal => {
                    if c.is_whitespace() {
                        let start = pos;
                        while pos < end_pos && chars[pos].is_whitespace() {
                            pos += 1;
                        }
                        tokens.push(Token {
                            kind: TokenKind::Whitespace,
                            text: chars[start..pos].iter().collect(),
                            start,
                            end: pos,
                        });
                        continue;
                    }

                    // Line comment: --
                    if c == '-' && pos + 1 < end_pos && chars[pos + 1] == '-' {
                        token_start = pos;
                        state = LexerState::InLineComment;
                        pos += 2;
                        continue;
                    }

                    // Block comment: /*
                    if c == '/' && pos + 1 < end_pos && chars[pos + 1] == '*' {
                        token_start = pos;
                        state = LexerState::InBlockComment;
                        pos += 2;
                        continue;
                    }

                    // Escape string: E'...'
                    if (c == 'E' || c == 'e') && pos + 1 < end_pos && chars[pos + 1] == '\'' {
                        token_start = pos;
                        state = LexerState::InEscapeString;
                        pos += 2;
                        continue;
                    }

                    // Dollar-quoted string: $tag$...$tag$ or $$...$$
                    if c == '$' {
                        let tag_start = pos;
                        pos += 1;
                        let mut tag = String::new();
                        while pos < end_pos && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                            tag.push(chars[pos]);
                            pos += 1;
                        }
                        if pos < end_pos && chars[pos] == '$' {
                            pos += 1;
                            token_start = tag_start;
                            dollar_tag = tag;
                            state = LexerState::InDollarQuote;
                            continue;
                        } else {
                            // Not a valid dollar quote, treat as operator
                            tokens.push(Token {
                                kind: TokenKind::Operator("$".to_string()),
                                text: "$".to_string(),
                                start: tag_start,
                                end: tag_start + 1,
                            });
                            // Reprocess characters after $
                            pos = tag_start + 1;
                            continue;
                        }
                    }

                    // Single-quoted string: '...'
                    if c == '\'' {
                        token_start = pos;
                        state = LexerState::InSingleQuote;
                        pos += 1;
                        continue;
                    }

                    // Double-quoted identifier: "..."
                    if c == '"' {
                        token_start = pos;
                        state = LexerState::InDoubleQuote;
                        pos += 1;
                        continue;
                    }

                    // Cast operator: ::
                    if c == ':' && pos + 1 < end_pos && chars[pos + 1] == ':' {
                        tokens.push(Token {
                            kind: TokenKind::Operator("::".to_string()),
                            text: "::".to_string(),
                            start: pos,
                            end: pos + 2,
                        });
                        pos += 2;
                        continue;
                    }

                    // Other operators
                    if Self::is_operator_char(c) {
                        let start = pos;
                        let mut op = String::new();
                        while pos < end_pos && Self::is_operator_char(chars[pos]) {
                            op.push(chars[pos]);
                            pos += 1;
                        }
                        tokens.push(Token {
                            kind: TokenKind::Operator(op.clone()),
                            text: op,
                            start,
                            end: pos,
                        });
                        continue;
                    }

                    // Punctuation: ( ) , ; . [ ]
                    if Self::is_punctuation(c) {
                        tokens.push(Token {
                            kind: TokenKind::Punctuation(c),
                            text: c.to_string(),
                            start: pos,
                            end: pos + 1,
                        });
                        pos += 1;
                        continue;
                    }

                    // Number
                    if c.is_ascii_digit() {
                        let start = pos;
                        while pos < end_pos && (chars[pos].is_ascii_digit() || chars[pos] == '.') {
                            pos += 1;
                        }
                        tokens.push(Token {
                            kind: TokenKind::Number,
                            text: chars[start..pos].iter().collect(),
                            start,
                            end: pos,
                        });
                        continue;
                    }

                    // Identifier or keyword
                    if c.is_alphabetic() || c == '_' {
                        let start = pos;
                        while pos < end_pos && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                            pos += 1;
                        }
                        let text: String = chars[start..pos].iter().collect();
                        let upper = text.to_uppercase();
                        let kind = if SQL_KEYWORDS.contains(&upper.as_str()) {
                            TokenKind::Keyword(upper)
                        } else {
                            TokenKind::Identifier(text.clone())
                        };
                        tokens.push(Token {
                            kind,
                            text,
                            start,
                            end: pos,
                        });
                        continue;
                    }

                    // Unknown character
                    tokens.push(Token {
                        kind: TokenKind::Unknown,
                        text: c.to_string(),
                        start: pos,
                        end: pos + 1,
                    });
                    pos += 1;
                }

                LexerState::InSingleQuote => {
                    // Handle escaped single quotes: ''
                    if c == '\'' {
                        if pos + 1 < end_pos && chars[pos + 1] == '\'' {
                            pos += 2;
                            continue;
                        }
                        // End of string
                        tokens.push(Token {
                            kind: TokenKind::StringLiteral,
                            text: chars[token_start..=pos].iter().collect(),
                            start: token_start,
                            end: pos + 1,
                        });
                        state = LexerState::Normal;
                        pos += 1;
                        continue;
                    }
                    pos += 1;
                }

                LexerState::InDoubleQuote => {
                    // Handle escaped double quotes: ""
                    if c == '"' {
                        if pos + 1 < end_pos && chars[pos + 1] == '"' {
                            pos += 2;
                            continue;
                        }
                        // End of identifier
                        let text: String = chars[token_start..=pos].iter().collect();
                        tokens.push(Token {
                            kind: TokenKind::Identifier(text.clone()),
                            text,
                            start: token_start,
                            end: pos + 1,
                        });
                        state = LexerState::Normal;
                        pos += 1;
                        continue;
                    }
                    pos += 1;
                }

                LexerState::InDollarQuote => {
                    // Look for closing $tag$
                    if c == '$' {
                        let tag_start = pos;
                        pos += 1;
                        let mut closing_tag = String::new();
                        while pos < end_pos && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                            closing_tag.push(chars[pos]);
                            pos += 1;
                        }
                        if pos < end_pos && chars[pos] == '$' && closing_tag == dollar_tag {
                            pos += 1;
                            tokens.push(Token {
                                kind: TokenKind::StringLiteral,
                                text: chars[token_start..pos].iter().collect(),
                                start: token_start,
                                end: pos,
                            });
                            state = LexerState::Normal;
                            dollar_tag.clear();
                            continue;
                        } else {
                            // Not closing tag, continue in dollar quote
                            pos = tag_start + 1;
                            continue;
                        }
                    }
                    pos += 1;
                }

                LexerState::InLineComment => {
                    if c == '\n' {
                        tokens.push(Token {
                            kind: TokenKind::Comment,
                            text: chars[token_start..pos].iter().collect(),
                            start: token_start,
                            end: pos,
                        });
                        state = LexerState::Normal;
                        // Don't consume newline, let Normal state handle it
                        continue;
                    }
                    pos += 1;
                }

                LexerState::InBlockComment => {
                    if c == '*' && pos + 1 < end_pos && chars[pos + 1] == '/' {
                        pos += 2;
                        tokens.push(Token {
                            kind: TokenKind::Comment,
                            text: chars[token_start..pos].iter().collect(),
                            start: token_start,
                            end: pos,
                        });
                        state = LexerState::Normal;
                        continue;
                    }
                    pos += 1;
                }

                LexerState::InEscapeString => {
                    // Handle backslash escapes in E'...'
                    if c == '\\' && pos + 1 < end_pos {
                        pos += 2;
                        continue;
                    }
                    if c == '\'' {
                        tokens.push(Token {
                            kind: TokenKind::StringLiteral,
                            text: chars[token_start..=pos].iter().collect(),
                            start: token_start,
                            end: pos + 1,
                        });
                        state = LexerState::Normal;
                        pos += 1;
                        continue;
                    }
                    pos += 1;
                }
            }
        }

        // Handle unterminated tokens at cursor position
        if state != LexerState::Normal {
            let text: String = chars[token_start..end_pos].iter().collect();
            let kind = match state {
                LexerState::InSingleQuote
                | LexerState::InDollarQuote
                | LexerState::InEscapeString => TokenKind::StringLiteral,
                LexerState::InDoubleQuote => TokenKind::Identifier(text.clone()),
                LexerState::InLineComment | LexerState::InBlockComment => TokenKind::Comment,
                LexerState::Normal => unreachable!(),
            };
            tokens.push(Token {
                kind,
                text,
                start: token_start,
                end: end_pos,
            });
        }

        tokens
    }

    pub fn is_in_string_or_comment(&self, text: &str, cursor_pos: usize) -> bool {
        let tokens = self.tokenize(text, cursor_pos, None);

        if let Some(last) = tokens.last() {
            // If cursor is at the end of the last token
            if last.end == cursor_pos {
                matches!(last.kind, TokenKind::StringLiteral | TokenKind::Comment)
            } else if last.start <= cursor_pos && cursor_pos < last.end {
                // Cursor is inside a token
                matches!(last.kind, TokenKind::StringLiteral | TokenKind::Comment)
            } else {
                false
            }
        } else {
            false
        }
    }

    fn is_operator_char(c: char) -> bool {
        matches!(
            c,
            '+' | '-' | '*' | '/' | '<' | '>' | '=' | '!' | '%' | '&' | '|' | '^' | '~' | ':'
        )
    }

    fn is_punctuation(c: char) -> bool {
        matches!(c, '(' | ')' | ',' | ';' | '.' | '[' | ']')
    }

    pub fn extract_table_references(&self, tokens: &[Token]) -> Vec<TableReference> {
        let mut refs = Vec::new();
        let mut i = 0;
        let mut prev_keyword: Option<&str> = None;
        // Track FOR locking clause: FOR [NO KEY | KEY]? (UPDATE | SHARE)
        let mut in_for_clause = false;

        while i < tokens.len() {
            let token = &tokens[i];

            // Reset state on statement terminator
            if token.kind == TokenKind::Punctuation(';') {
                in_for_clause = false;
                prev_keyword = None;
                i += 1;
                continue;
            }

            if let TokenKind::Keyword(kw) = &token.kind {
                match kw.as_str() {
                    "FROM" | "JOIN" => {
                        in_for_clause = false;
                        prev_keyword = Some(kw.as_str());
                        i += 1;
                        while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                            i += 1;
                        }
                        // Skip ONLY keyword (PostgreSQL inheritance)
                        if i < tokens.len()
                            && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                        {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                        }
                        if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                            refs.push(table_ref);
                            continue;
                        }
                    }
                    // JOIN modifiers - skip to find JOIN, then parse table
                    "INNER" | "LEFT" | "RIGHT" | "FULL" | "CROSS" => {
                        in_for_clause = false;
                        prev_keyword = Some(kw.as_str());
                        i += 1;
                        while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                            i += 1;
                        }
                        // Check for JOIN keyword
                        if i < tokens.len()
                            && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "JOIN")
                        {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                            if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                                refs.push(table_ref);
                                continue;
                            }
                        }
                    }
                    // FOR starts a locking clause (FOR UPDATE, FOR NO KEY UPDATE, etc.)
                    "FOR" => {
                        in_for_clause = true;
                        prev_keyword = Some("FOR");
                    }
                    // NO, KEY, SHARE are part of FOR locking clause
                    "NO" | "KEY" | "SHARE" if in_for_clause => {
                        prev_keyword = Some(kw.as_str());
                    }
                    // UPDATE: skip if in FOR locking clause
                    "UPDATE" if !in_for_clause => {
                        prev_keyword = Some("UPDATE");
                        i += 1;
                        while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                            i += 1;
                        }
                        // Skip ONLY keyword (PostgreSQL inheritance)
                        if i < tokens.len()
                            && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                        {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                        }
                        if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                            refs.push(table_ref);
                            continue;
                        }
                    }
                    // INSERT INTO table_name ... (only after INSERT, not SELECT INTO)
                    "INTO" if prev_keyword == Some("INSERT") => {
                        i += 1;
                        while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                            i += 1;
                        }
                        // Skip ONLY keyword (PostgreSQL inheritance)
                        if i < tokens.len()
                            && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                        {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                        }
                        if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                            refs.push(table_ref);
                            continue;
                        }
                    }
                    other => {
                        in_for_clause = false;
                        prev_keyword = Some(other);
                    }
                }
            }
            i += 1;
        }

        refs
    }

    fn parse_table_reference(&self, tokens: &[Token], i: &mut usize) -> Option<TableReference> {
        if *i >= tokens.len() {
            return None;
        }

        let position = tokens[*i].start;
        let mut schema = None;
        let mut table;
        let mut alias = None;

        // Get first identifier (could be schema or table)
        match &tokens[*i].kind {
            TokenKind::Identifier(name) | TokenKind::Keyword(name) => {
                table = name.clone();
            }
            _ => return None,
        }
        *i += 1;

        // Skip whitespace
        while *i < tokens.len() && tokens[*i].kind == TokenKind::Whitespace {
            *i += 1;
        }

        // Check for schema.table pattern
        if *i < tokens.len() && tokens[*i].kind == TokenKind::Punctuation('.') {
            *i += 1;
            // Skip whitespace
            while *i < tokens.len() && tokens[*i].kind == TokenKind::Whitespace {
                *i += 1;
            }
            if *i < tokens.len()
                && let TokenKind::Identifier(name) | TokenKind::Keyword(name) = &tokens[*i].kind
            {
                schema = Some(table);
                table = name.clone();
                *i += 1;
            }
        }

        // Skip whitespace
        while *i < tokens.len() && tokens[*i].kind == TokenKind::Whitespace {
            *i += 1;
        }

        // Check for alias (optional AS keyword)
        if *i < tokens.len()
            && let TokenKind::Keyword(kw) = &tokens[*i].kind
            && kw == "AS"
        {
            *i += 1;
            // Skip whitespace
            while *i < tokens.len() && tokens[*i].kind == TokenKind::Whitespace {
                *i += 1;
            }
        }

        // Get alias if present (identifier that's not a keyword like ON, WHERE, etc.)
        if *i < tokens.len() {
            match &tokens[*i].kind {
                TokenKind::Identifier(name) => {
                    alias = Some(name.clone());
                    *i += 1;
                }
                TokenKind::Keyword(kw) => {
                    // Don't treat SQL keywords as aliases
                    if !Self::is_clause_keyword(kw) {
                        alias = Some(kw.clone());
                        *i += 1;
                    }
                }
                _ => {}
            }
        }

        Some(TableReference {
            schema,
            table,
            alias,
            position,
        })
    }

    fn is_clause_keyword(kw: &str) -> bool {
        matches!(
            kw,
            "SELECT"
                | "FROM"
                | "WHERE"
                | "JOIN"
                | "ON"
                | "AND"
                | "OR"
                | "ORDER"
                | "GROUP"
                | "HAVING"
                | "LIMIT"
                | "OFFSET"
                | "UNION"
                | "INTERSECT"
                | "EXCEPT"
                | "LEFT"
                | "RIGHT"
                | "INNER"
                | "OUTER"
                | "CROSS"
                | "FULL"
                | "NATURAL"
        )
    }

    pub fn extract_cte_definitions(&self, tokens: &[Token]) -> Vec<CteDefinition> {
        let mut ctes = Vec::new();
        let mut i = 0;

        while i < tokens.len() {
            let token = &tokens[i];

            // Look for WITH keyword
            if let TokenKind::Keyword(kw) = &token.kind
                && kw == "WITH"
            {
                i += 1;

                // Skip RECURSIVE if present
                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                    i += 1;
                }
                if i < tokens.len()
                    && let TokenKind::Keyword(k) = &tokens[i].kind
                    && k == "RECURSIVE"
                {
                    i += 1;
                }

                // Parse CTE definitions separated by commas
                loop {
                    // Skip whitespace
                    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                        i += 1;
                    }

                    if i >= tokens.len() {
                        break;
                    }

                    // Get CTE name
                    let position = tokens[i].start;
                    if let TokenKind::Identifier(name) | TokenKind::Keyword(name) = &tokens[i].kind
                    {
                        // Don't treat SELECT as a CTE name
                        if name != "SELECT" {
                            ctes.push(CteDefinition {
                                name: name.clone(),
                                position,
                            });
                        }
                        i += 1;

                        // Skip until we find AS or comma or SELECT
                        let mut paren_depth = 0;
                        while i < tokens.len() {
                            match &tokens[i].kind {
                                TokenKind::Punctuation('(') => paren_depth += 1,
                                TokenKind::Punctuation(')') => {
                                    if paren_depth > 0 {
                                        paren_depth -= 1;
                                    }
                                }
                                TokenKind::Punctuation(',') if paren_depth == 0 => {
                                    i += 1;
                                    break;
                                }
                                TokenKind::Keyword(k) if k == "SELECT" && paren_depth == 0 => {
                                    // End of CTE definitions
                                    return ctes;
                                }
                                _ => {}
                            }
                            i += 1;
                        }
                    } else {
                        break;
                    }
                }
            }
            i += 1;
        }

        ctes
    }

    pub fn build_context(&self, tokens: &[Token], cursor_pos: usize) -> SqlContext {
        let tables = self.extract_table_references(tokens);
        let ctes = self.extract_cte_definitions(tokens);
        let current_clause = self.detect_clause_at_cursor(tokens, cursor_pos);
        let target_table = self.extract_target_table(tokens, cursor_pos);

        SqlContext {
            tables,
            ctes,
            current_clause,
            target_table,
        }
    }

    /// Finds semicolon positions in the token stream
    /// Returns a list of indices where semicolons appear
    fn find_semicolon_positions(&self, tokens: &[Token]) -> Vec<usize> {
        tokens
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                if t.kind == TokenKind::Punctuation(';') {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Determines which statement (delimited by semicolons) the cursor belongs to
    /// Returns (start_token_index, end_token_index) of the statement
    fn find_statement_range(&self, tokens: &[Token], cursor_pos: usize) -> (usize, usize) {
        let semicolons = self.find_semicolon_positions(tokens);

        if semicolons.is_empty() {
            // Single statement - entire token stream
            return (0, tokens.len());
        }

        // Find which statement the cursor belongs to
        let mut start = 0;
        for &semi_idx in &semicolons {
            if semi_idx >= tokens.len() {
                break;
            }
            let semi_pos = tokens[semi_idx].end;
            if cursor_pos <= semi_pos {
                // Cursor is before or at this semicolon
                return (start, semi_idx + 1);
            }
            start = semi_idx + 1;
        }

        // Cursor is after the last semicolon
        (start, tokens.len())
    }

    /// Extracts the target table for UPDATE/DELETE/INSERT statements
    /// Handles WITH clauses by scanning for statement-level mutation keywords
    /// Now scans only the statement where cursor_pos is located
    fn extract_target_table(&self, tokens: &[Token], cursor_pos: usize) -> Option<TableReference> {
        // Find the range of tokens for the statement containing the cursor
        let (start_idx, end_idx) = self.find_statement_range(tokens, cursor_pos);

        let mut i = start_idx;
        let mut paren_depth: i32 = 0;
        // Track FOR locking clause: FOR [NO KEY | KEY]? (UPDATE | SHARE)
        let mut in_for_clause = false;

        while i < end_idx {
            let token = &tokens[i];

            match &token.kind {
                TokenKind::Punctuation(p) if *p == '(' => paren_depth += 1,
                TokenKind::Punctuation(p) if *p == ')' => {
                    paren_depth = paren_depth.saturating_sub(1)
                }
                // Reset state on statement terminator
                TokenKind::Punctuation(p) if *p == ';' => {
                    in_for_clause = false;
                }
                TokenKind::Keyword(kw) if paren_depth == 0 => {
                    match kw.as_str() {
                        // FOR starts a locking clause
                        "FOR" => {
                            in_for_clause = true;
                        }
                        // NO, KEY, SHARE are part of FOR locking clause
                        "NO" | "KEY" | "SHARE" if in_for_clause => {}
                        // UPDATE: skip if in FOR locking clause
                        "UPDATE" if in_for_clause => {
                            in_for_clause = false;
                        }
                        "UPDATE" => {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                            // Skip ONLY keyword (PostgreSQL inheritance)
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                            {
                                i += 1;
                                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                    i += 1;
                                }
                            }
                            return self.parse_table_reference(tokens, &mut i);
                        }
                        "DELETE" => {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                            // Skip FROM if present
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "FROM")
                            {
                                i += 1;
                                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                    i += 1;
                                }
                            }
                            // Skip ONLY keyword (PostgreSQL inheritance)
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                            {
                                i += 1;
                                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                    i += 1;
                                }
                            }
                            return self.parse_table_reference(tokens, &mut i);
                        }
                        "INSERT" => {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                            // Skip INTO if present
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "INTO")
                            {
                                i += 1;
                                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                    i += 1;
                                }
                            }
                            // Skip ONLY keyword (PostgreSQL inheritance)
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                            {
                                i += 1;
                                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                    i += 1;
                                }
                            }
                            return self.parse_table_reference(tokens, &mut i);
                        }
                        _ => {
                            in_for_clause = false;
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }

        None
    }

    fn detect_clause_at_cursor(&self, tokens: &[Token], cursor_pos: usize) -> ClauseKind {
        let mut last_clause = ClauseKind::Unknown;

        for token in tokens {
            if token.start > cursor_pos {
                break;
            }

            if let TokenKind::Keyword(kw) = &token.kind {
                last_clause = match kw.as_str() {
                    "SELECT" => ClauseKind::Select,
                    "FROM" => ClauseKind::From,
                    "JOIN" | "LEFT" | "RIGHT" | "INNER" | "OUTER" | "CROSS" | "FULL" => {
                        ClauseKind::Join
                    }
                    "WHERE" => ClauseKind::Where,
                    "ON" => ClauseKind::On,
                    "GROUP" => ClauseKind::GroupBy,
                    "ORDER" => ClauseKind::OrderBy,
                    "HAVING" => ClauseKind::Having,
                    "INSERT" | "INTO" => ClauseKind::InsertInto,
                    "UPDATE" | "SET" => ClauseKind::UpdateSet,
                    "WITH" => ClauseKind::With,
                    _ => last_clause,
                };
            }
        }

        last_clause
    }
}

impl Default for SqlLexer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexer() -> SqlLexer {
        SqlLexer::new()
    }

    mod tokenization {
        use super::*;

        #[test]
        fn simple_select_extracts_keywords() {
            let l = lexer();

            let tokens = l.tokenize("SELECT * FROM users", 19, None);

            let keywords: Vec<_> = tokens
                .iter()
                .filter_map(|t| match &t.kind {
                    TokenKind::Keyword(k) => Some(k.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(keywords, vec!["SELECT", "FROM"]);
        }

        #[test]
        fn non_keyword_returns_identifier() {
            let l = lexer();

            let tokens = l.tokenize("SELECT username FROM users", 26, None);

            let identifiers: Vec<_> = tokens
                .iter()
                .filter_map(|t| match &t.kind {
                    TokenKind::Identifier(id) => Some(id.as_str()),
                    _ => None,
                })
                .collect();
            assert!(identifiers.contains(&"username"));
            assert!(identifiers.contains(&"users"));
        }

        #[test]
        fn cast_operator_returns_operator_token() {
            let l = lexer();

            let tokens = l.tokenize("SELECT col::integer", 19, None);

            let has_cast = tokens
                .iter()
                .any(|t| matches!(&t.kind, TokenKind::Operator(op) if op == "::"));
            assert!(has_cast);
        }

        #[test]
        fn array_access_returns_punctuation_tokens() {
            let l = lexer();

            let tokens = l.tokenize("SELECT arr[0]", 13, None);

            let punctuations: Vec<_> = tokens
                .iter()
                .filter_map(|t| match &t.kind {
                    TokenKind::Punctuation(c) => Some(*c),
                    _ => None,
                })
                .collect();
            assert!(punctuations.contains(&'['));
            assert!(punctuations.contains(&']'));
        }
    }

    mod string_literals {
        use super::*;

        #[test]
        fn single_quoted_string_returns_string_literal() {
            let l = lexer();

            let tokens = l.tokenize("SELECT 'hello'", 14, None);

            let has_string = tokens.iter().any(|t| t.kind == TokenKind::StringLiteral);
            assert!(has_string);
        }

        #[test]
        fn keyword_in_string_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("SELECT 'SELECT'", 15, None);

            let keywords: Vec<_> = tokens
                .iter()
                .filter_map(|t| match &t.kind {
                    TokenKind::Keyword(k) => Some(k.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(keywords.len(), 1);
            assert_eq!(keywords[0], "SELECT");
        }

        #[test]
        fn escaped_single_quote_returns_single_literal() {
            let l = lexer();

            let tokens = l.tokenize("SELECT 'O''Brien'", 17, None);

            let string_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| t.kind == TokenKind::StringLiteral)
                .collect();
            assert_eq!(string_tokens.len(), 1);
            assert_eq!(string_tokens[0].text, "'O''Brien'");
        }

        #[test]
        fn dollar_quoted_string_returns_string_literal() {
            let l = lexer();

            let tokens = l.tokenize("SELECT $$hello$$", 16, None);

            let has_string = tokens.iter().any(|t| t.kind == TokenKind::StringLiteral);
            assert!(has_string);
        }

        #[test]
        fn keyword_in_dollar_quote_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("SELECT $$SELECT$$", 17, None);

            let keywords: Vec<_> = tokens
                .iter()
                .filter_map(|t| match &t.kind {
                    TokenKind::Keyword(k) => Some(k.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(keywords.len(), 1);
        }

        #[test]
        fn tagged_dollar_quote_returns_string_literal() {
            let l = lexer();

            let tokens = l.tokenize("SELECT $tag$SELECT$tag$", 23, None);

            let string_tokens: Vec<_> = tokens
                .iter()
                .filter(|t| t.kind == TokenKind::StringLiteral)
                .collect();
            assert_eq!(string_tokens.len(), 1);
            assert_eq!(string_tokens[0].text, "$tag$SELECT$tag$");
        }

        #[test]
        fn escape_string_returns_string_literal() {
            let l = lexer();

            let tokens = l.tokenize("SELECT E'hello\\nworld'", 22, None);

            let has_string = tokens.iter().any(|t| t.kind == TokenKind::StringLiteral);
            assert!(has_string);
        }
    }

    mod comments {
        use super::*;

        #[test]
        fn line_comment_returns_comment_token() {
            let l = lexer();

            let tokens = l.tokenize("SELECT -- comment\n* FROM", 24, None);

            let has_comment = tokens.iter().any(|t| t.kind == TokenKind::Comment);
            assert!(has_comment);
        }

        #[test]
        fn keyword_in_line_comment_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("-- SELECT\nFROM", 14, None);

            let keywords: Vec<_> = tokens
                .iter()
                .filter_map(|t| match &t.kind {
                    TokenKind::Keyword(k) => Some(k.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(keywords, vec!["FROM"]);
        }

        #[test]
        fn block_comment_returns_comment_token() {
            let l = lexer();

            let tokens = l.tokenize("SELECT /* comment */ * FROM", 27, None);

            let has_comment = tokens.iter().any(|t| t.kind == TokenKind::Comment);
            assert!(has_comment);
        }

        #[test]
        fn keyword_in_block_comment_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("/* SELECT */ FROM", 17, None);

            let keywords: Vec<_> = tokens
                .iter()
                .filter_map(|t| match &t.kind {
                    TokenKind::Keyword(k) => Some(k.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(keywords, vec!["FROM"]);
        }
    }

    mod cursor_context {
        use super::*;

        #[test]
        fn cursor_in_string_returns_true() {
            let l = lexer();

            let result = l.is_in_string_or_comment("SELECT 'hel", 11);

            assert!(result);
        }

        #[test]
        fn cursor_in_line_comment_returns_true() {
            let l = lexer();

            let result = l.is_in_string_or_comment("SELECT -- com", 13);

            assert!(result);
        }

        #[test]
        fn cursor_in_block_comment_returns_true() {
            let l = lexer();

            let result = l.is_in_string_or_comment("SELECT /* com", 13);

            assert!(result);
        }

        #[test]
        fn cursor_in_normal_context_returns_false() {
            let l = lexer();

            let result = l.is_in_string_or_comment("SELECT * FROM ", 14);

            assert!(!result);
        }

        #[test]
        fn cursor_after_closed_string_returns_false() {
            let l = lexer();

            let result = l.is_in_string_or_comment("SELECT 'hello' FROM ", 20);

            assert!(!result);
        }
    }

    mod cache {
        use super::*;

        #[test]
        fn valid_cache_returns_cached_tokens() {
            let l = lexer();
            let content = "SELECT * FROM users";
            let cursor = 19;
            let tokens1 = l.tokenize(content, cursor, None);
            let mut cache = TokenCache::new();
            cache.update(content, cursor, tokens1.clone());

            let tokens2 = l.tokenize(content, cursor, Some(&cache));

            assert_eq!(tokens1.len(), tokens2.len());
        }

        #[test]
        fn content_change_invalidates_cache() {
            let l = lexer();
            let mut cache = TokenCache::new();
            let tokens1 = l.tokenize("SELECT", 6, None);
            cache.update("SELECT", 6, tokens1);

            let is_valid = cache.is_valid("SELECT *", 8);

            assert!(!is_valid);
        }

        #[test]
        fn cursor_change_invalidates_cache() {
            let l = lexer();
            let mut cache = TokenCache::new();
            let tokens1 = l.tokenize("SELECT * FROM", 13, None);
            cache.update("SELECT * FROM", 13, tokens1);

            let is_valid = cache.is_valid("SELECT * FROM", 7);

            assert!(!is_valid);
        }
    }

    mod table_references {
        use super::*;

        #[test]
        fn simple_from_returns_single_reference() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users", 19, None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, None);
            assert_eq!(refs[0].schema, None);
        }

        #[test]
        fn from_with_alias_returns_alias() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users u", 21, None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
        }

        #[test]
        fn from_with_as_keyword_returns_alias() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users AS u", 24, None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
        }

        #[test]
        fn schema_qualified_table_returns_schema() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM public.users", 26, None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].schema, Some("public".to_string()));
            assert_eq!(refs[0].table, "users");
        }

        #[test]
        fn join_returns_multiple_references() {
            let l = lexer();
            let tokens = l.tokenize(
                "SELECT * FROM users u JOIN posts p ON u.id = p.user_id",
                54,
                None,
            );

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
            assert_eq!(refs[1].table, "posts");
            assert_eq!(refs[1].alias, Some("p".to_string()));
        }

        #[test]
        fn left_join_returns_reference() {
            let l = lexer();
            let tokens = l.tokenize(
                "SELECT * FROM users LEFT JOIN posts ON users.id = posts.user_id",
                63,
                None,
            );

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[1].table, "posts");
        }

        #[test]
        fn multiple_joins_returns_all_references() {
            let l = lexer();
            let sql = "SELECT * FROM users u JOIN posts p ON u.id = p.user_id JOIN comments c ON p.id = c.post_id";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 3);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[1].table, "posts");
            assert_eq!(refs[2].table, "comments");
        }
    }

    mod cte_definitions {
        use super::*;

        #[test]
        fn simple_cte_returns_definition() {
            let l = lexer();
            let sql = "WITH active_users AS (SELECT * FROM users WHERE active) SELECT * FROM active_users";
            let tokens = l.tokenize(sql, sql.len(), None);

            let ctes = l.extract_cte_definitions(&tokens);

            assert_eq!(ctes.len(), 1);
            assert_eq!(ctes[0].name, "active_users");
        }

        #[test]
        fn recursive_cte_returns_definition() {
            let l = lexer();
            let sql = "WITH RECURSIVE tree AS (SELECT 1) SELECT * FROM tree";
            let tokens = l.tokenize(sql, sql.len(), None);

            let ctes = l.extract_cte_definitions(&tokens);

            assert_eq!(ctes.len(), 1);
            assert_eq!(ctes[0].name, "tree");
        }

        #[test]
        fn multiple_ctes_returns_all_definitions() {
            let l = lexer();
            let sql = "WITH cte1 AS (SELECT 1), cte2 AS (SELECT 2) SELECT * FROM cte1, cte2";
            let tokens = l.tokenize(sql, sql.len(), None);

            let ctes = l.extract_cte_definitions(&tokens);

            assert_eq!(ctes.len(), 2);
            assert_eq!(ctes[0].name, "cte1");
            assert_eq!(ctes[1].name, "cte2");
        }

        #[test]
        fn no_cte_returns_empty() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users", 19, None);

            let ctes = l.extract_cte_definitions(&tokens);

            assert!(ctes.is_empty());
        }
    }

    mod clause_detection {
        use super::*;

        #[test]
        fn cursor_after_select_returns_select_clause() {
            let l = lexer();
            let sql = "SELECT ";
            let tokens = l.tokenize(sql, sql.len(), None);

            let clause = l.detect_clause_at_cursor(&tokens, sql.len());

            assert_eq!(clause, ClauseKind::Select);
        }

        #[test]
        fn cursor_after_from_returns_from_clause() {
            let l = lexer();
            let sql = "SELECT * FROM ";
            let tokens = l.tokenize(sql, sql.len(), None);

            let clause = l.detect_clause_at_cursor(&tokens, sql.len());

            assert_eq!(clause, ClauseKind::From);
        }

        #[test]
        fn cursor_after_where_returns_where_clause() {
            let l = lexer();
            let sql = "SELECT * FROM users WHERE ";
            let tokens = l.tokenize(sql, sql.len(), None);

            let clause = l.detect_clause_at_cursor(&tokens, sql.len());

            assert_eq!(clause, ClauseKind::Where);
        }

        #[test]
        fn cursor_after_join_returns_join_clause() {
            let l = lexer();
            let sql = "SELECT * FROM users JOIN ";
            let tokens = l.tokenize(sql, sql.len(), None);

            let clause = l.detect_clause_at_cursor(&tokens, sql.len());

            assert_eq!(clause, ClauseKind::Join);
        }

        #[test]
        fn cursor_after_with_returns_with_clause() {
            let l = lexer();
            let sql = "WITH ";
            let tokens = l.tokenize(sql, sql.len(), None);

            let clause = l.detect_clause_at_cursor(&tokens, sql.len());

            assert_eq!(clause, ClauseKind::With);
        }
    }

    mod build_context {
        use super::*;

        #[test]
        fn full_query_returns_complete_context() {
            let l = lexer();
            let sql = "WITH cte AS (SELECT 1) SELECT * FROM users u JOIN posts p ON u.id = p.user_id WHERE ";
            let tokens = l.tokenize(sql, sql.len(), None);

            let ctx = l.build_context(&tokens, sql.len());

            assert_eq!(ctx.ctes.len(), 1);
            assert_eq!(ctx.tables.len(), 2);
            assert_eq!(ctx.current_clause, ClauseKind::Where);
        }
    }

    mod target_table {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("UPDATE users SET name = 'foo'", Some("users"))]
        #[case("DELETE FROM orders WHERE id = 1", Some("orders"))]
        #[case("INSERT INTO posts (title) VALUES ('test')", Some("posts"))]
        #[case("SELECT * FROM users", None)]
        fn extract_target_returns_expected(#[case] sql: &str, #[case] expected: Option<&str>) {
            let l = lexer();
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, sql.len());

            assert_eq!(target.as_ref().map(|t| t.table.as_str()), expected);
        }

        #[rstest]
        #[case("UPDATE users SET name = 'foo'", "users")]
        #[case("INSERT INTO posts (title) VALUES ('test')", "posts")]
        #[case("DELETE FROM orders WHERE id = 1", "orders")]
        fn mutation_table_in_references(#[case] sql: &str, #[case] expected: &str) {
            let l = lexer();
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, expected);
        }

        #[test]
        fn with_clause_update_extracts_target() {
            let l = lexer();
            let sql = "WITH active AS (SELECT id FROM users WHERE active) UPDATE users SET status = 'inactive'";
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, sql.len());

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "users");
        }

        #[test]
        fn for_update_is_not_target() {
            let l = lexer();
            let sql = "SELECT * FROM users FOR UPDATE";
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, sql.len());

            assert!(target.is_none());
        }

        #[test]
        fn select_into_not_in_references() {
            let l = lexer();
            let sql = "SELECT * INTO new_table FROM users";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            // Only "users" should be included, not "new_table"
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
        }

        #[test]
        fn for_update_not_in_references() {
            let l = lexer();
            let sql = "SELECT * FROM users FOR UPDATE";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            // Only "users" should be included, FOR UPDATE should not add a reference
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
        }

        #[test]
        fn for_no_key_update_not_in_references() {
            let l = lexer();
            let sql = "SELECT * FROM users FOR NO KEY UPDATE";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
        }

        #[test]
        fn for_no_key_update_is_not_target() {
            let l = lexer();
            let sql = "SELECT * FROM users FOR NO KEY UPDATE";
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, sql.len());

            assert!(target.is_none());
        }

        #[test]
        fn multi_statement_for_share_then_update_extracts_both_tables() {
            let l = lexer();
            let sql = "SELECT * FROM users FOR SHARE; UPDATE orders SET status = 'done'";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[1].table, "orders");
        }

        #[test]
        fn multi_statement_for_update_then_update_extracts_both_tables() {
            let l = lexer();
            let sql = "SELECT * FROM users FOR UPDATE; UPDATE orders SET status = 'done'";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[1].table, "orders");
        }

        #[test]
        fn multi_statement_for_no_key_update_then_update_extracts_both_tables() {
            let l = lexer();
            let sql = "SELECT * FROM users FOR NO KEY UPDATE; UPDATE orders SET status = 'done'";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[1].table, "orders");
        }

        #[test]
        fn update_only_skips_only_keyword() {
            let l = lexer();
            let sql = "UPDATE ONLY users SET name = 'foo'";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
        }

        #[test]
        fn update_only_target_table() {
            let l = lexer();
            let sql = "UPDATE ONLY users SET name = 'foo'";
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, sql.len());

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "users");
        }

        #[test]
        fn delete_from_only_skips_only_keyword() {
            let l = lexer();
            let sql = "DELETE FROM ONLY orders WHERE id = 1";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "orders");
        }

        #[test]
        fn delete_from_only_target_table() {
            let l = lexer();
            let sql = "DELETE FROM ONLY orders WHERE id = 1";
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, sql.len());

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "orders");
        }

        #[test]
        fn insert_into_only_skips_only_keyword() {
            let l = lexer();
            let sql = "INSERT INTO ONLY posts (title) VALUES ('test')";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "posts");
        }

        #[test]
        fn insert_into_only_target_table() {
            let l = lexer();
            let sql = "INSERT INTO ONLY posts (title) VALUES ('test')";
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, sql.len());

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "posts");
        }

        #[test]
        fn select_from_only_skips_only_keyword() {
            let l = lexer();
            let sql = "SELECT * FROM ONLY users WHERE active = true";
            let tokens = l.tokenize(sql, sql.len(), None);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
        }

        #[test]
        fn multi_statement_cursor_in_first_update() {
            let l = lexer();
            let sql = "UPDATE users SET x = 1; UPDATE orders SET y = 2";
            // Cursor at position 10 (in "users")
            let cursor_pos = 10;
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, cursor_pos);

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "users");
        }

        #[test]
        fn multi_statement_cursor_in_second_update() {
            let l = lexer();
            let sql = "UPDATE users SET x = 1; UPDATE orders SET y = 2";
            // Cursor at position 35 (in "orders")
            let cursor_pos = 35;
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, cursor_pos);

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "orders");
        }

        #[test]
        fn multi_statement_cursor_at_end_of_second_update() {
            let l = lexer();
            let sql = "UPDATE users SET x = 1; UPDATE orders SET y = 2";
            // Cursor at end of SQL
            let cursor_pos = sql.len();
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, cursor_pos);

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "orders");
        }

        #[test]
        fn multi_statement_select_then_update_cursor_in_select() {
            let l = lexer();
            let sql = "SELECT * FROM users; UPDATE orders SET status = 'done'";
            // Cursor at position 10 (in SELECT statement)
            let cursor_pos = 10;
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, cursor_pos);

            // SELECT has no target table
            assert!(target.is_none());
        }

        #[test]
        fn multi_statement_select_then_update_cursor_in_update() {
            let l = lexer();
            let sql = "SELECT * FROM users; UPDATE orders SET status = 'done'";
            // Cursor at position 30 (in UPDATE statement)
            let cursor_pos = 30;
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, cursor_pos);

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "orders");
        }

        #[test]
        fn multi_statement_three_statements_cursor_in_middle() {
            let l = lexer();
            let sql = "UPDATE users SET x = 1; DELETE FROM posts WHERE id = 1; INSERT INTO orders (status) VALUES ('new')";
            // Cursor at position 40 (in DELETE statement)
            let cursor_pos = 40;
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, cursor_pos);

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "posts");
        }

        #[test]
        fn multi_statement_three_statements_cursor_in_last() {
            let l = lexer();
            let sql = "UPDATE users SET x = 1; DELETE FROM posts WHERE id = 1; INSERT INTO orders (status) VALUES ('new')";
            // Cursor at position 80 (in INSERT statement)
            let cursor_pos = 80;
            let tokens = l.tokenize(sql, sql.len(), None);

            let target = l.extract_target_table(&tokens, cursor_pos);

            assert!(target.is_some());
            assert_eq!(target.unwrap().table, "orders");
        }
    }
}
