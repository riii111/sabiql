use crate::domain::DatabaseType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(String),
    Identifier(String),
    BacktickIdentifier(String),
    Operator(String),
    Punctuation(char),
    StringLiteral,
    Number,
    Comment,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReference {
    pub schema: Option<String>,
    pub table: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SqlContext {
    pub tables: Vec<TableReference>,
    pub ctes: Vec<String>,
    // Target table for UPDATE/DELETE/INSERT statements (for column priority boost)
    pub target_table: Option<TableReference>,
}

const MYSQL_INSERT_MODIFIERS: &[&str] = &["LOW_PRIORITY", "DELAYED", "HIGH_PRIORITY", "IGNORE"];
const MYSQL_UPDATE_MODIFIERS: &[&str] = &["LOW_PRIORITY", "IGNORE"];
const MYSQL_DELETE_MODIFIERS: &[&str] = &["LOW_PRIORITY", "QUICK", "IGNORE"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexerState {
    Normal,
    InSingleQuote,
    InDoubleQuoteIdentifier,
    InDoubleQuoteString,
    InBacktickIdentifier,
    InDollarQuote,
    InLineComment,
    InBlockComment,
    InEscapeString,
}

pub(crate) const POSTGRESQL_KEYWORDS: &[&str] = &[
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
    "DO",
    "GRANT",
    "REVOKE",
    "COPY",
    "CALL",
    "MERGE",
    "TRUNCATE",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "EXPLAIN",
    "ANALYZE",
    "SHOW",
    "SAVEPOINT",
    "START",
    "TRANSACTION",
    "RELEASE",
];

pub(crate) const MYSQL_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "JOIN",
    "STRAIGHT_JOIN",
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
    "GROUP",
    "HAVING",
    "LIMIT",
    "OFFSET",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "ALL",
    "INSERT",
    "REPLACE",
    "INTO",
    "VALUES",
    "CALL",
    "UPDATE",
    "SET",
    "DELETE",
    "CREATE",
    "DROP",
    "TRUNCATE",
    "ALTER",
    "TABLE",
    "INDEX",
    "VIEW",
    "WITH",
    "RECURSIVE",
    "COALESCE",
    "NULLIF",
    "CAST",
    "USING",
    "FULL",
    "NATURAL",
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
    "EXPLAIN",
    "ANALYZE",
    "SHOW",
    "DESCRIBE",
    "DATABASE",
    "DATABASES",
    "PRIMARY",
    "KEY",
    "FOREIGN",
    "REFERENCES",
    "UNIQUE",
    "DEFAULT",
    "CONSTRAINT",
    "CHECK",
    "IF",
    "CASCADE",
    "RENAME",
    "MODIFY",
    "COLUMN",
    "ENGINE",
    "CHARACTER",
    "CHARSET",
    "COLLATE",
    "AUTO_INCREMENT",
    "SAVEPOINT",
    "RELEASE",
    "USE",
    "FOR",
    "LOCK",
    "SHARE",
    "START",
    "TRANSACTION",
    "COMMIT",
    "ROLLBACK",
];

pub struct SqlLexer {
    database_type: DatabaseType,
}

impl SqlLexer {
    pub fn new(database_type: DatabaseType) -> Self {
        Self { database_type }
    }

    fn is_mysql(&self) -> bool {
        self.database_type == DatabaseType::MySQL
    }

    fn is_keyword(&self, word: &str) -> bool {
        let keywords = if self.is_mysql() {
            MYSQL_KEYWORDS
        } else {
            POSTGRESQL_KEYWORDS
        };
        keywords.contains(&word)
    }

    pub fn tokenize(&self, text: &str, cursor_pos: usize) -> Vec<Token> {
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
                        while pos < end_pos && chars[pos].is_whitespace() {
                            pos += 1;
                        }
                        continue;
                    }

                    // MySQL starts a -- comment only when followed by ASCII whitespace or EOF.
                    if c == '-'
                        && pos + 1 < end_pos
                        && chars[pos + 1] == '-'
                        && (!self.is_mysql()
                            || pos + 2 == end_pos
                            || chars[pos + 2].is_ascii_whitespace())
                    {
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

                    if self.is_mysql() && c == '#' {
                        token_start = pos;
                        state = LexerState::InLineComment;
                        pos += 1;
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
                        }
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

                    // Single-quoted string: '...'
                    if c == '\'' {
                        token_start = pos;
                        state = LexerState::InSingleQuote;
                        pos += 1;
                        continue;
                    }

                    // MySQL treats double quotes as strings; PostgreSQL treats them as identifiers.
                    if c == '"' {
                        token_start = pos;
                        state = if self.is_mysql() {
                            LexerState::InDoubleQuoteString
                        } else {
                            LexerState::InDoubleQuoteIdentifier
                        };
                        pos += 1;
                        continue;
                    }

                    if self.is_mysql() && c == '`' {
                        token_start = pos;
                        state = LexerState::InBacktickIdentifier;
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
                        let kind = if self.is_keyword(&upper) {
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
                    if self.is_mysql() && c == '\\' && pos + 1 < end_pos {
                        pos += 2;
                        continue;
                    }
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

                LexerState::InDoubleQuoteIdentifier => {
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

                LexerState::InDoubleQuoteString => {
                    if c == '\\' && pos + 1 < end_pos {
                        pos += 2;
                        continue;
                    }
                    // Handle escaped double quotes: ""
                    if c == '"' {
                        if pos + 1 < end_pos && chars[pos + 1] == '"' {
                            pos += 2;
                            continue;
                        }
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

                LexerState::InBacktickIdentifier => {
                    if c == '`' {
                        if pos + 1 < end_pos && chars[pos + 1] == '`' {
                            pos += 2;
                            continue;
                        }
                        let name: String = chars[token_start + 1..pos]
                            .iter()
                            .collect::<String>()
                            .replace("``", "`");
                        tokens.push(Token {
                            kind: TokenKind::BacktickIdentifier(name),
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
                        }
                        // Not closing tag, continue in dollar quote
                        pos = tag_start + 1;
                        continue;
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
                | LexerState::InDoubleQuoteString
                | LexerState::InDollarQuote
                | LexerState::InEscapeString => TokenKind::StringLiteral,
                LexerState::InDoubleQuoteIdentifier => TokenKind::Identifier(text.clone()),
                LexerState::InBacktickIdentifier => {
                    TokenKind::BacktickIdentifier(text[1..].to_string())
                }
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

    pub fn is_in_string_or_comment_from_tokens(tokens: &[Token], cursor_pos: usize) -> bool {
        tokens.iter().any(|t| {
            if matches!(t.kind, TokenKind::StringLiteral | TokenKind::Comment) {
                t.start < cursor_pos && cursor_pos <= t.end
            } else if matches!(t.kind, TokenKind::BacktickIdentifier(_)) {
                (t.start < cursor_pos && cursor_pos < t.end)
                    || (cursor_pos == t.end && Self::is_unterminated_backtick(&t.text))
            } else {
                false
            }
        })
    }

    fn is_unterminated_backtick(text: &str) -> bool {
        if !text.starts_with('`') {
            return false;
        }
        let mut chars = text.chars().skip(1).peekable();
        while let Some(ch) = chars.next() {
            if ch != '`' {
                continue;
            }
            if chars.next_if_eq(&'`').is_none() {
                return false;
            }
        }
        true
    }

    fn is_operator_char(c: char) -> bool {
        matches!(
            c,
            '+' | '-' | '*' | '/' | '<' | '>' | '=' | '!' | '%' | '&' | '|' | '^' | '~' | ':'
        )
    }

    fn is_mysql_index_hint_scope(&self, tokens: &[Token], scope_index: usize) -> bool {
        if !self.is_mysql() {
            return false;
        }

        let is_scope = matches!(
            &tokens[scope_index].kind,
            TokenKind::Keyword(word)
                if matches!(word.as_str(), "JOIN" | "ORDER" | "GROUP")
        );
        if !is_scope {
            return false;
        }

        let Some(for_index) = scope_index.checked_sub(1) else {
            return false;
        };
        let is_for = matches!(
            &tokens[for_index].kind,
            TokenKind::Keyword(word) | TokenKind::Identifier(word)
                if word.eq_ignore_ascii_case("FOR")
        );
        if !is_for {
            return false;
        }

        let Some(index_hint_index) = for_index.checked_sub(1) else {
            return false;
        };
        matches!(
            &tokens[index_hint_index].kind,
            TokenKind::Keyword(word) | TokenKind::Identifier(word)
                if word.eq_ignore_ascii_case("INDEX") || word.eq_ignore_ascii_case("KEY")
        )
    }

    fn is_punctuation(c: char) -> bool {
        matches!(c, '(' | ')' | ',' | ';' | '.' | '[' | ']')
    }

    fn skip_mysql_modifiers(
        &self,
        tokens: &[Token],
        mut index: usize,
        modifiers: &[&str],
    ) -> usize {
        if !self.is_mysql() {
            return index;
        }

        loop {
            let Some(token) = tokens.get(index) else {
                return index;
            };
            let is_modifier = matches!(
                &token.kind,
                TokenKind::Identifier(_) | TokenKind::Keyword(_)
            ) && modifiers
                .iter()
                .any(|modifier| token.text.eq_ignore_ascii_case(modifier));
            if !is_modifier {
                return index;
            }
            index += 1;
        }
    }

    fn skip_only_keyword(tokens: &[Token], mut index: usize) -> usize {
        if index < tokens.len()
            && matches!(&tokens[index].kind, TokenKind::Keyword(k) if k == "ONLY")
        {
            index += 1;
        }
        index
    }

    fn is_mysql_upsert_update(&self, tokens: &[Token], update_index: usize) -> bool {
        if !self.is_mysql() {
            return false;
        }

        let mut index = update_index;
        for expected in ["KEY", "DUPLICATE", "ON"] {
            let Some(previous_index) = index.checked_sub(1) else {
                return false;
            };
            index = previous_index;
            let token = &tokens[index];
            let is_expected = matches!(
                &token.kind,
                TokenKind::Keyword(word) | TokenKind::Identifier(word)
                    if word.eq_ignore_ascii_case(expected)
            );
            if !is_expected {
                return false;
            }
        }

        true
    }

    pub fn extract_table_references(&self, tokens: &[Token]) -> Vec<TableReference> {
        let mut refs = Vec::new();
        let mut i = 0;
        // Track FOR locking clause: FOR [NO KEY | KEY]? (UPDATE | SHARE)
        let mut in_for_clause = false;
        let mut can_start_straight_join = false;

        while i < tokens.len() {
            let token = &tokens[i];

            // Reset state on statement terminator
            if token.kind == TokenKind::Punctuation(';') {
                in_for_clause = false;
                can_start_straight_join = false;
                i += 1;
                continue;
            }

            if let TokenKind::Keyword(kw) = &token.kind {
                match kw.as_str() {
                    "FROM" | "JOIN" | "STRAIGHT_JOIN" => {
                        if kw == "JOIN" && self.is_mysql_index_hint_scope(tokens, i) {
                            i += 1;
                            continue;
                        }
                        if kw == "STRAIGHT_JOIN" && !can_start_straight_join {
                            i += 1;
                            continue;
                        }
                        in_for_clause = false;
                        can_start_straight_join = false;
                        i += 1;
                        // Skip ONLY keyword (PostgreSQL inheritance)
                        i = Self::skip_only_keyword(tokens, i);
                        if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                            refs.push(table_ref);
                            can_start_straight_join = true;
                            continue;
                        }
                    }
                    // JOIN modifiers - skip to find JOIN, then parse table
                    "INNER" | "LEFT" | "RIGHT" | "FULL" | "CROSS" => {
                        in_for_clause = false;
                        can_start_straight_join = false;
                        i += 1;
                        // Check for JOIN keyword
                        if i < tokens.len()
                            && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "JOIN")
                        {
                            i += 1;
                            if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                                refs.push(table_ref);
                                can_start_straight_join = true;
                                continue;
                            }
                        }
                    }
                    // FOR starts a locking clause (FOR UPDATE, FOR NO KEY UPDATE, etc.)
                    "FOR" => {
                        let next = i + 1;
                        if next >= tokens.len() || !self.is_mysql_index_hint_scope(tokens, next) {
                            in_for_clause = true;
                            can_start_straight_join = false;
                        }
                    }
                    // NO, KEY, SHARE are part of FOR locking clause
                    "NO" | "KEY" | "SHARE" if in_for_clause => {}
                    "GROUP" | "ORDER" if self.is_mysql_index_hint_scope(tokens, i) => {
                        i += 1;
                        continue;
                    }
                    "INSERT" | "REPLACE" => {
                        in_for_clause = false;
                        can_start_straight_join = false;
                        i += 1;
                        i = self.skip_mysql_modifiers(tokens, i, MYSQL_INSERT_MODIFIERS);
                        if i < tokens.len()
                            && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "INTO")
                        {
                            i += 1;
                        }
                        if i < tokens.len()
                            && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                        {
                            i += 1;
                        }
                        if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                            refs.push(table_ref);
                            continue;
                        }
                    }
                    "UPDATE" if self.is_mysql_upsert_update(tokens, i) => {
                        can_start_straight_join = false;
                        i += 1;
                        continue;
                    }
                    // UPDATE: skip if in FOR locking clause
                    "UPDATE" if !in_for_clause => {
                        can_start_straight_join = false;
                        i += 1;
                        i = self.skip_mysql_modifiers(tokens, i, MYSQL_UPDATE_MODIFIERS);
                        // Skip ONLY keyword (PostgreSQL inheritance)
                        i = Self::skip_only_keyword(tokens, i);
                        if let Some(table_ref) = self.parse_table_reference(tokens, &mut i) {
                            refs.push(table_ref);
                            can_start_straight_join = true;
                            continue;
                        }
                    }
                    "SELECT" | "WHERE" | "GROUP" | "ORDER" | "HAVING" | "LIMIT" | "OFFSET"
                    | "SET" | "UNION" | "INTERSECT" | "EXCEPT" => {
                        in_for_clause = false;
                        can_start_straight_join = false;
                    }
                    _ => {
                        in_for_clause = false;
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

        let mut schema = None;
        let mut table;
        let mut alias = None;

        // Get first identifier (could be schema or table)
        table = Self::identifier_value(&tokens[*i])?;
        *i += 1;

        // Check for schema.table pattern
        if *i < tokens.len() && tokens[*i].kind == TokenKind::Punctuation('.') {
            *i += 1;
            let token = tokens.get(*i)?;
            if let Some(name) = Self::identifier_value(token) {
                schema = Some(table);
                table = name;
                *i += 1;
            } else {
                return None;
            }
        }

        if self.is_mysql()
            && *i < tokens.len()
            && matches!(&tokens[*i].kind, TokenKind::Keyword(kw) if kw == "PARTITION")
        {
            let partition_start = *i + 1;
            if partition_start < tokens.len()
                && tokens[partition_start].kind == TokenKind::Punctuation('(')
            {
                *i = partition_start;
                let mut partition_depth = 0;
                while *i < tokens.len() {
                    match tokens[*i].kind {
                        TokenKind::Punctuation('(') => partition_depth += 1,
                        TokenKind::Punctuation(')') => {
                            partition_depth -= 1;
                        }
                        _ => {}
                    }
                    *i += 1;
                    if partition_depth == 0 {
                        break;
                    }
                }
            }
        }

        // Check for alias (optional AS keyword)
        if *i < tokens.len()
            && let TokenKind::Keyword(kw) = &tokens[*i].kind
            && kw == "AS"
        {
            *i += 1;
        }

        // Get alias if present (identifier that's not a keyword like ON, WHERE, etc.)
        if *i < tokens.len() {
            let is_clause_keyword = matches!(
                &tokens[*i].kind,
                TokenKind::Keyword(kw) if Self::is_clause_keyword(kw)
            );
            if !is_clause_keyword && let Some(name) = Self::identifier_value(&tokens[*i]) {
                alias = Some(name);
                *i += 1;
            }
        }

        Some(TableReference {
            schema,
            table,
            alias,
        })
    }

    fn identifier_value(token: &Token) -> Option<String> {
        match &token.kind {
            TokenKind::Identifier(_)
                if token.text.starts_with('"') && token.text.ends_with('"') =>
            {
                Some(token.text[1..token.text.len() - 1].replace("\"\"", "\""))
            }
            TokenKind::Identifier(name)
            | TokenKind::BacktickIdentifier(name)
            | TokenKind::Keyword(name) => Some(name.clone()),
            _ => None,
        }
    }

    fn is_clause_keyword(kw: &str) -> bool {
        matches!(
            kw,
            "SELECT"
                | "FROM"
                | "WHERE"
                | "JOIN"
                | "STRAIGHT_JOIN"
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

    pub fn extract_cte_definitions(&self, tokens: &[Token]) -> Vec<String> {
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
                if i < tokens.len()
                    && let TokenKind::Keyword(k) = &tokens[i].kind
                    && k == "RECURSIVE"
                {
                    i += 1;
                }

                // Parse CTE definitions separated by commas
                loop {
                    if i >= tokens.len() {
                        break;
                    }

                    // Get CTE name
                    if let Some(name) = Self::identifier_value(&tokens[i]) {
                        // Don't treat the SELECT keyword as a CTE name. A quoted
                        // identifier named SELECT is a valid CTE name.
                        let is_select_keyword = matches!(
                            &tokens[i].kind,
                            TokenKind::Keyword(keyword) if keyword == "SELECT"
                        );
                        if !is_select_keyword {
                            ctes.push(name);
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
        let tokens = self.tokens_for_statement(tokens, cursor_pos);
        self.build_context_from_tokens(tokens, cursor_pos)
    }

    pub(crate) fn build_context_before_cursor(
        &self,
        tokens: &[Token],
        cursor_pos: usize,
    ) -> SqlContext {
        let tokens = self.tokens_for_statement_before_cursor(tokens, cursor_pos);
        self.build_context_from_tokens(tokens, cursor_pos)
    }

    fn build_context_from_tokens(&self, tokens: &[Token], cursor_pos: usize) -> SqlContext {
        let tables = self.extract_table_references(tokens);
        let ctes = self.extract_cte_definitions(tokens);
        let target_table = self.extract_target_table(tokens, cursor_pos);

        SqlContext {
            tables,
            ctes,
            target_table,
        }
    }

    pub(crate) fn tokens_for_statement<'a>(
        &self,
        tokens: &'a [Token],
        cursor_pos: usize,
    ) -> &'a [Token] {
        let (start_idx, end_idx) = self.find_statement_range(tokens, cursor_pos);
        &tokens[start_idx..end_idx]
    }

    pub(crate) fn tokens_for_statement_before_cursor<'a>(
        &self,
        tokens: &'a [Token],
        cursor_pos: usize,
    ) -> &'a [Token] {
        let statement_tokens = self.tokens_for_statement(tokens, cursor_pos);
        let end_idx = statement_tokens
            .iter()
            .position(|token| token.end > cursor_pos)
            .unwrap_or(statement_tokens.len());

        &statement_tokens[..end_idx]
    }

    fn find_statement_range(&self, tokens: &[Token], cursor_pos: usize) -> (usize, usize) {
        let mut start = 0;
        for (index, token) in tokens.iter().enumerate() {
            if token.kind == TokenKind::Punctuation(';') {
                if cursor_pos < token.end {
                    return (start, index + 1);
                }
                start = index + 1;
            }
        }

        (start, tokens.len())
    }

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
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
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
                            i = self.skip_mysql_modifiers(tokens, i, MYSQL_UPDATE_MODIFIERS);
                            // Skip ONLY keyword (PostgreSQL inheritance)
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "ONLY")
                            {
                                i += 1;
                            }
                            return self.parse_table_reference(tokens, &mut i);
                        }
                        "DELETE" => {
                            i += 1;
                            i = self.skip_mysql_modifiers(tokens, i, MYSQL_DELETE_MODIFIERS);
                            // Skip FROM if present
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "FROM")
                            {
                                i += 1;
                            }
                            // Skip ONLY keyword (PostgreSQL inheritance)
                            i = Self::skip_only_keyword(tokens, i);
                            return self.parse_table_reference(tokens, &mut i);
                        }
                        "INSERT" | "REPLACE" => {
                            i += 1;
                            i = self.skip_mysql_modifiers(tokens, i, MYSQL_INSERT_MODIFIERS);
                            // Skip INTO if present
                            if i < tokens.len()
                                && matches!(&tokens[i].kind, TokenKind::Keyword(k) if k == "INTO")
                            {
                                i += 1;
                            }
                            // Skip ONLY keyword (PostgreSQL inheritance)
                            i = Self::skip_only_keyword(tokens, i);
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
}

impl Default for SqlLexer {
    fn default() -> Self {
        Self::new(DatabaseType::PostgreSQL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexer() -> SqlLexer {
        SqlLexer::default()
    }

    mod tokenization {
        use super::*;

        #[test]
        fn simple_select_extracts_keywords() {
            let l = lexer();

            let tokens = l.tokenize("SELECT * FROM users", 19);

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
        fn whitespace_advances_cursor_without_tokens_or_span_changes() {
            let tokens = lexer().tokenize("SELECT * FROM users", 19);

            let spans: Vec<_> = tokens
                .iter()
                .map(|token| (token.text.as_str(), token.start, token.end))
                .collect();

            assert_eq!(
                spans,
                vec![
                    ("SELECT", 0, 6),
                    ("*", 7, 8),
                    ("FROM", 9, 13),
                    ("users", 14, 19),
                ]
            );
        }

        #[test]
        fn statement_keywords_tokenize_as_keywords() {
            let l = lexer();

            for kw in [
                "DO",
                "GRANT",
                "REVOKE",
                "COPY",
                "CALL",
                "MERGE",
                "TRUNCATE",
                "BEGIN",
                "COMMIT",
                "ROLLBACK",
                "EXPLAIN",
                "ANALYZE",
                "SHOW",
                "SAVEPOINT",
                "START",
                "TRANSACTION",
                "RELEASE",
            ] {
                let sql = format!("{kw} x");
                let tokens = l.tokenize(&sql, sql.chars().count());

                assert!(
                    matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == kw),
                    "{kw} should tokenize as a keyword"
                );
            }
        }

        #[test]
        fn non_keyword_returns_identifier() {
            let l = lexer();

            let tokens = l.tokenize("SELECT username FROM users", 26);

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

            let tokens = l.tokenize("SELECT col::integer", 19);

            let has_cast = tokens
                .iter()
                .any(|t| matches!(&t.kind, TokenKind::Operator(op) if op == "::"));
            assert!(has_cast);
        }

        #[test]
        fn array_access_returns_punctuation_tokens() {
            let l = lexer();

            let tokens = l.tokenize("SELECT arr[0]", 13);

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

            let tokens = l.tokenize("SELECT 'hello'", 14);

            let has_string = tokens.iter().any(|t| t.kind == TokenKind::StringLiteral);
            assert!(has_string);
        }

        #[test]
        fn keyword_in_string_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("SELECT 'SELECT'", 15);

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

            let tokens = l.tokenize("SELECT 'O''Brien'", 17);

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

            let tokens = l.tokenize("SELECT $$hello$$", 16);

            let has_string = tokens.iter().any(|t| t.kind == TokenKind::StringLiteral);
            assert!(has_string);
        }

        #[test]
        fn keyword_in_dollar_quote_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("SELECT $$SELECT$$", 17);

            assert_eq!(
                tokens
                    .iter()
                    .filter_map(|t| match &t.kind {
                        TokenKind::Keyword(k) => Some(k.as_str()),
                        _ => None,
                    })
                    .count(),
                1
            );
        }

        #[test]
        fn tagged_dollar_quote_returns_string_literal() {
            let l = lexer();

            let tokens = l.tokenize("SELECT $tag$SELECT$tag$", 23);

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

            let tokens = l.tokenize("SELECT E'hello\\nworld'", 22);

            let has_string = tokens.iter().any(|t| t.kind == TokenKind::StringLiteral);
            assert!(has_string);
        }
    }

    mod comments {
        use super::*;

        #[test]
        fn line_comment_returns_comment_token() {
            let l = lexer();

            let tokens = l.tokenize("SELECT -- comment\n* FROM", 24);

            let has_comment = tokens.iter().any(|t| t.kind == TokenKind::Comment);
            assert!(has_comment);
        }

        #[test]
        fn keyword_in_line_comment_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("-- SELECT\nFROM", 14);

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

            let tokens = l.tokenize("SELECT /* comment */ * FROM", 27);

            let has_comment = tokens.iter().any(|t| t.kind == TokenKind::Comment);
            assert!(has_comment);
        }

        #[test]
        fn keyword_in_block_comment_returns_only_outer_keyword() {
            let l = lexer();

            let tokens = l.tokenize("/* SELECT */ FROM", 17);

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

    mod mysql_syntax {
        use super::*;

        fn mysql_lexer() -> SqlLexer {
            SqlLexer::new(DatabaseType::MySQL)
        }

        #[test]
        fn backtick_identifiers_are_normalized_and_preserve_escaped_ticks() {
            let l = mysql_lexer();
            let sql = "SELECT `select` FROM `app`.`order``items`";
            let tokens = l.tokenize(sql, sql.chars().count());

            assert!(tokens.iter().any(|token| {
                matches!(
                    &token.kind,
                    TokenKind::BacktickIdentifier(name) if name == "select"
                )
            }));
            assert!(tokens.iter().any(|token| {
                matches!(
                    &token.kind,
                    TokenKind::BacktickIdentifier(name) if name == "order`items"
                )
            }));
        }

        #[test]
        fn hash_comment_hides_keywords_from_context() {
            let l = mysql_lexer();
            let tokens = l.tokenize("SELECT # FROM\nFROM", 18);
            let keywords: Vec<_> = tokens
                .iter()
                .filter_map(|token| match &token.kind {
                    TokenKind::Keyword(keyword) => Some(keyword.as_str()),
                    _ => None,
                })
                .collect();

            assert_eq!(keywords, vec!["SELECT", "FROM"]);
        }

        #[test]
        fn mysql_keywords_are_distinct_from_postgresql_keywords() {
            let mysql = mysql_lexer();
            let postgres = SqlLexer::default();

            assert!(matches!(
                mysql.tokenize("DESCRIBE users", 14)[0].kind,
                TokenKind::Keyword(_)
            ));
            assert!(matches!(
                postgres.tokenize("DESCRIBE users", 14)[0].kind,
                TokenKind::Identifier(_)
            ));
        }

        #[test]
        fn mysql_statement_keywords_are_displayed_as_keywords() {
            let l = mysql_lexer();

            for word in ["TRUNCATE", "REPLACE", "CALL", "SAVEPOINT", "RELEASE", "USE"] {
                assert!(matches!(
                    l.tokenize(word, word.len())[0].kind,
                    TokenKind::Keyword(_)
                ));
            }
        }

        #[test]
        fn backtick_identifier_is_not_a_completion_context() {
            let l = mysql_lexer();
            let sql = "SELECT `SEL";

            assert!(SqlLexer::is_in_string_or_comment_from_tokens(
                &l.tokenize(sql, sql.chars().count()),
                sql.chars().count()
            ));
            assert!(SqlLexer::is_in_string_or_comment_from_tokens(
                &l.tokenize("SELECT `a``", "SELECT `a``".len()),
                11
            ));
            assert!(SqlLexer::is_in_string_or_comment_from_tokens(
                &l.tokenize("SELECT `a``b`", "SELECT `a``b`".len()),
                10
            ));
            assert!(!SqlLexer::is_in_string_or_comment_from_tokens(
                &l.tokenize("SELECT `SEL`", "SELECT `SEL`".len()),
                12
            ));
        }

        #[test]
        fn double_quoted_mysql_string_is_not_an_identifier_context() {
            let l = mysql_lexer();
            let sql = r#"SELECT "users.""#;
            let tokens = l.tokenize(sql, sql.chars().count());

            assert!(tokens.iter().any(|token| {
                matches!(
                    &token.kind,
                    TokenKind::StringLiteral if token.text == r#""users.""#
                )
            }));
            assert!(SqlLexer::is_in_string_or_comment_from_tokens(
                &tokens,
                sql.chars().count()
            ));
            assert!(!SqlLexer::is_in_string_or_comment_from_tokens(
                &l.tokenize(r#"SELECT "users." FROM "#, 20),
                20
            ));
        }

        #[test]
        fn mysql_backslash_escape_keeps_following_table_context() {
            let l = mysql_lexer();
            let sql = r"SELECT 'it\'s' AS label FROM us";
            let tokens = l.tokenize(sql, sql.chars().count());

            assert!(tokens.iter().any(|token| {
                matches!(
                    &token.kind,
                    TokenKind::StringLiteral if token.text == "'it\\'s'"
                )
            }));
            assert!(tokens.iter().any(|token| {
                matches!(&token.kind, TokenKind::Keyword(keyword) if keyword == "FROM")
            }));
            assert!(!SqlLexer::is_in_string_or_comment_from_tokens(
                &tokens,
                sql.chars().count()
            ));
            assert_eq!(l.extract_table_references(&tokens)[0].table, "us");
        }

        #[test]
        fn mysql_double_dash_requires_whitespace_or_end_of_input() {
            let l = mysql_lexer();
            let tokens = l.tokenize("SELECT 1--1 FROM users", 22);

            assert!(!tokens.iter().any(|token| token.kind == TokenKind::Comment));
            assert!(tokens.iter().any(|token| {
                matches!(&token.kind, TokenKind::Keyword(keyword) if keyword == "FROM")
            }));
            assert!(
                l.tokenize("SELECT 1 -- comment", 19)
                    .iter()
                    .any(|token| token.kind == TokenKind::Comment)
            );
            assert!(
                l.tokenize("SELECT 1--", 10)
                    .iter()
                    .any(|token| token.kind == TokenKind::Comment)
            );
        }

        #[test]
        fn backtick_qualified_table_reference_supports_aliases() {
            let l = mysql_lexer();
            let sql = "SELECT * FROM `app`.`users` AS `u`";
            let tokens = l.tokenize(sql, sql.chars().count());
            let references = l.extract_table_references(&tokens);

            assert_eq!(
                references,
                vec![TableReference {
                    schema: Some("app".to_string()),
                    table: "users".to_string(),
                    alias: Some("u".to_string()),
                }]
            );
        }
    }

    mod cursor_context {
        use super::*;

        #[test]
        fn cursor_in_string_returns_true() {
            let l = lexer();
            let sql = "SELECT 'hel";

            let result =
                SqlLexer::is_in_string_or_comment_from_tokens(&l.tokenize(sql, sql.len()), 11);

            assert!(result);
        }

        #[test]
        fn cursor_in_line_comment_returns_true() {
            let l = lexer();
            let sql = "SELECT -- com";

            let result =
                SqlLexer::is_in_string_or_comment_from_tokens(&l.tokenize(sql, sql.len()), 13);

            assert!(result);
        }

        #[test]
        fn cursor_in_block_comment_returns_true() {
            let l = lexer();
            let sql = "SELECT /* com";

            let result =
                SqlLexer::is_in_string_or_comment_from_tokens(&l.tokenize(sql, sql.len()), 13);

            assert!(result);
        }

        #[test]
        fn cursor_in_normal_context_returns_false() {
            let l = lexer();
            let sql = "SELECT * FROM ";

            let result =
                SqlLexer::is_in_string_or_comment_from_tokens(&l.tokenize(sql, sql.len()), 14);

            assert!(!result);
        }

        #[test]
        fn cursor_after_closed_string_returns_false() {
            let l = lexer();
            let sql = "SELECT 'hello' FROM ";

            let result =
                SqlLexer::is_in_string_or_comment_from_tokens(&l.tokenize(sql, sql.len()), 20);

            assert!(!result);
        }
    }

    mod table_references {
        use super::*;

        #[test]
        fn simple_from_returns_single_reference() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users", 19);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, None);
            assert_eq!(refs[0].schema, None);
        }

        #[test]
        fn from_with_alias_returns_alias() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users u", 21);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
        }

        #[test]
        fn from_with_as_keyword_returns_alias() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users AS u", 24);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
        }

        #[test]
        fn schema_qualified_table_returns_schema() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM public.users", 26);

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].schema, Some("public".to_string()));
            assert_eq!(refs[0].table, "users");
        }

        #[test]
        fn double_quoted_schema_and_table_preserve_case_and_embedded_dots() {
            let l = lexer();
            let sql = r#"SELECT * FROM "Sales.Region"."Orders.Archive""#;
            let tokens = l.tokenize(sql, sql.chars().count());

            assert_eq!(
                l.extract_table_references(&tokens),
                vec![TableReference {
                    schema: Some("Sales.Region".to_string()),
                    table: "Orders.Archive".to_string(),
                    alias: None,
                }]
            );
        }

        #[test]
        fn sqlite_double_quoted_schema_and_table_preserve_identity() {
            let l = SqlLexer::new(DatabaseType::SQLite);
            let sql = r#"SELECT * FROM "Sales.Region"."Orders.Archive""#;
            let tokens = l.tokenize(sql, sql.chars().count());

            assert_eq!(
                l.extract_table_references(&tokens),
                vec![TableReference {
                    schema: Some("Sales.Region".to_string()),
                    table: "Orders.Archive".to_string(),
                    alias: None,
                }]
            );
        }

        #[test]
        fn join_returns_multiple_references() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users u JOIN posts p ON u.id = p.user_id", 54);

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
            let tokens = l.tokenize(sql, sql.len());

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 3);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[1].table, "posts");
            assert_eq!(refs[2].table, "comments");
        }

        #[test]
        fn mysql_straight_join_returns_joined_reference() {
            let l = SqlLexer::new(DatabaseType::MySQL);
            let sql = "SELECT * FROM users u STRAIGHT_JOIN orders o ON u.id = o.user_id";
            let tokens = l.tokenize(sql, sql.len());

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
            assert_eq!(refs[1].table, "orders");
            assert_eq!(refs[1].alias, Some("o".to_string()));
        }

        #[test]
        fn mysql_partition_clause_preserves_table_alias() {
            let l = SqlLexer::new(DatabaseType::MySQL);
            let sql = "SELECT * FROM events PARTITION (p0) AS e WHERE e.id = 1";
            let tokens = l.tokenize(sql, sql.len());

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "events");
            assert_eq!(refs[0].alias, Some("e".to_string()));
        }

        #[test]
        fn mysql_straight_join_select_modifier_does_not_create_reference() {
            let l = SqlLexer::new(DatabaseType::MySQL);
            let sql = "SELECT STRAIGHT_JOIN id FROM users id WHERE id.id = 1";
            let tokens = l.tokenize(sql, sql.len());

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("id".to_string()));
        }

        #[test]
        fn mysql_straight_join_after_join_condition_returns_reference() {
            let l = SqlLexer::new(DatabaseType::MySQL);
            let sql = "SELECT * FROM users u STRAIGHT_JOIN orders o ON u.id = o.user_id STRAIGHT_JOIN items i ON i.order_id = o.id WHERE i.id = 1";
            let tokens = l.tokenize(sql, sql.len());

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 3);
            assert_eq!(refs[0].alias, Some("u".to_string()));
            assert_eq!(refs[1].alias, Some("o".to_string()));
            assert_eq!(refs[2].alias, Some("i".to_string()));
        }

        #[test]
        fn mysql_update_straight_join_returns_joined_reference() {
            let l = SqlLexer::new(DatabaseType::MySQL);
            let sql =
                "UPDATE users u STRAIGHT_JOIN orders o ON u.id = o.user_id SET o.status = 'done'";
            let tokens = l.tokenize(sql, sql.len());

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
            assert_eq!(refs[1].table, "orders");
            assert_eq!(refs[1].alias, Some("o".to_string()));
        }

        #[test]
        fn mysql_index_hint_for_join_preserves_straight_join() {
            let l = SqlLexer::new(DatabaseType::MySQL);
            let sql = "SELECT * FROM users u USE INDEX FOR JOIN (idx_users) STRAIGHT_JOIN orders o WHERE o.id = 1";
            let tokens = l.tokenize(sql, sql.len());

            let refs = l.extract_table_references(&tokens);

            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].table, "users");
            assert_eq!(refs[0].alias, Some("u".to_string()));
            assert_eq!(refs[1].table, "orders");
            assert_eq!(refs[1].alias, Some("o".to_string()));
        }

        #[test]
        fn mysql_index_hint_scopes_preserve_straight_join() {
            let l = SqlLexer::new(DatabaseType::MySQL);

            for scope in ["ORDER BY", "GROUP BY"] {
                let sql = format!(
                    "SELECT * FROM users u USE INDEX FOR {scope} (idx_users) STRAIGHT_JOIN orders o WHERE o.id = 1"
                );
                let tokens = l.tokenize(&sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 2, "scope: {scope}");
                assert_eq!(refs[0].alias, Some("u".to_string()), "scope: {scope}");
                assert_eq!(refs[1].alias, Some("o".to_string()), "scope: {scope}");
            }
        }
    }

    mod cte_definitions {
        use super::*;

        #[test]
        fn simple_cte_returns_definition() {
            let l = lexer();
            let sql = "WITH active_users AS (SELECT * FROM users WHERE active) SELECT * FROM active_users";
            let tokens = l.tokenize(sql, sql.len());

            let ctes = l.extract_cte_definitions(&tokens);

            assert_eq!(ctes.len(), 1);
            assert_eq!(ctes[0], "active_users");
        }

        #[test]
        fn recursive_cte_returns_definition() {
            let l = lexer();
            let sql = "WITH RECURSIVE tree AS (SELECT 1) SELECT * FROM tree";
            let tokens = l.tokenize(sql, sql.len());

            let ctes = l.extract_cte_definitions(&tokens);

            assert_eq!(ctes.len(), 1);
            assert_eq!(ctes[0], "tree");
        }

        #[test]
        fn quoted_keyword_is_a_cte_name() {
            let l = lexer();
            let sql = r#"WITH "SELECT" AS (SELECT 1) SELECT * FROM "SELECT""#;
            let tokens = l.tokenize(sql, sql.len());

            assert_eq!(
                l.extract_cte_definitions(&tokens),
                vec!["SELECT".to_string()]
            );
            assert_eq!(
                l.extract_table_references(&tokens),
                vec![TableReference {
                    schema: None,
                    table: "SELECT".to_string(),
                    alias: None,
                }]
            );
        }

        #[test]
        fn multiple_ctes_returns_all_definitions() {
            let l = lexer();
            let sql = "WITH cte1 AS (SELECT 1), cte2 AS (SELECT 2) SELECT * FROM cte1, cte2";
            let tokens = l.tokenize(sql, sql.len());

            let ctes = l.extract_cte_definitions(&tokens);

            assert_eq!(ctes.len(), 2);
            assert_eq!(ctes[0], "cte1");
            assert_eq!(ctes[1], "cte2");
        }

        #[test]
        fn no_cte_returns_empty() {
            let l = lexer();
            let tokens = l.tokenize("SELECT * FROM users", 19);

            let ctes = l.extract_cte_definitions(&tokens);

            assert!(ctes.is_empty());
        }
    }

    mod build_context {
        use super::*;

        #[test]
        fn full_query_returns_complete_context() {
            let l = lexer();
            let sql = "WITH cte AS (SELECT 1) SELECT * FROM users u JOIN posts p ON u.id = p.user_id WHERE ";
            let tokens = l.tokenize(sql, sql.len());

            let ctx = l.build_context(&tokens, sql.len());

            assert_eq!(ctx.ctes.len(), 1);
            assert_eq!(ctx.tables.len(), 2);
        }

        #[test]
        fn current_statement_context_excludes_other_statements() {
            let sql = "SELECT * FROM first_table; WITH current_cte AS (SELECT * FROM public.nested_table) SELECT * FROM public.current_table current_alias WHERE current_alias.id IN (SELECT id FROM public.subquery_table); SELECT * FROM later_table";
            let cursor_pos = sql.find("; SELECT * FROM later_table").unwrap();

            for database_type in DatabaseType::all() {
                let lexer = SqlLexer::new(*database_type);
                let tokens = lexer.tokenize(sql, sql.len());
                let context = lexer.build_context(&tokens, cursor_pos);

                assert_eq!(
                    context.ctes.iter().map(String::as_str).collect::<Vec<_>>(),
                    vec!["current_cte"]
                );
                assert_eq!(
                    context
                        .tables
                        .iter()
                        .map(|table| {
                            (
                                table.schema.as_deref(),
                                table.table.as_str(),
                                table.alias.as_deref(),
                            )
                        })
                        .collect::<Vec<_>>(),
                    vec![
                        (Some("public"), "nested_table", None),
                        (Some("public"), "current_table", Some("current_alias")),
                        (Some("public"), "subquery_table", None),
                    ]
                );
            }
        }
    }

    mod target_table {
        use super::*;
        use rstest::rstest;

        mod basic_extraction {
            use super::*;

            #[rstest]
            #[case("UPDATE users SET name = 'foo'", Some("users"))]
            #[case("DELETE FROM orders WHERE id = 1", Some("orders"))]
            #[case("INSERT INTO posts (title) VALUES ('test')", Some("posts"))]
            #[case("SELECT * FROM users", None)]
            fn extracts_target_table(#[case] sql: &str, #[case] expected: Option<&str>) {
                let l = lexer();
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, sql.len());

                assert_eq!(target.as_ref().map(|t| t.table.as_str()), expected);
            }

            #[rstest]
            #[case("UPDATE users SET name = 'foo'", "users")]
            #[case("INSERT INTO posts (title) VALUES ('test')", "posts")]
            #[case("DELETE FROM orders WHERE id = 1", "orders")]
            fn mutation_table_in_references(#[case] sql: &str, #[case] expected: &str) {
                let l = lexer();
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, expected);
            }

            #[test]
            fn mysql_dml_modifiers_are_skipped_for_targets_and_references() {
                let l = SqlLexer::new(DatabaseType::MySQL);
                for (sql, expected) in [
                    (
                        "INSERT LOW_PRIORITY INTO users (name) VALUES ('foo')",
                        "users",
                    ),
                    ("INSERT DELAYED INTO users (name) VALUES ('foo')", "users"),
                    (
                        "INSERT HIGH_PRIORITY INTO users (name) VALUES ('foo')",
                        "users",
                    ),
                    ("INSERT IGNORE INTO users (name) VALUES ('foo')", "users"),
                    ("UPDATE LOW_PRIORITY users SET name = 'foo'", "users"),
                    ("UPDATE IGNORE users SET name = 'foo'", "users"),
                    ("DELETE LOW_PRIORITY FROM users WHERE id = 1", "users"),
                    ("DELETE QUICK FROM users WHERE id = 1", "users"),
                    ("DELETE IGNORE FROM users WHERE id = 1", "users"),
                ] {
                    let tokens = l.tokenize(sql, sql.len());

                    assert_eq!(
                        l.extract_target_table(&tokens, sql.len())
                            .as_ref()
                            .map(|table| table.table.as_str()),
                        Some(expected),
                        "target table for {sql}"
                    );
                    assert_eq!(
                        l.extract_table_references(&tokens)
                            .first()
                            .map(|table| table.table.as_str()),
                        Some(expected),
                        "table reference for {sql}"
                    );
                }
            }

            #[test]
            fn mysql_replace_target_is_extracted() {
                let l = SqlLexer::new(DatabaseType::MySQL);
                for sql in [
                    "REPLACE INTO users (name) VALUES ('Ada')",
                    "REPLACE users (name) VALUES ('Ada')",
                    "INSERT users (name) VALUES ('Ada')",
                ] {
                    let tokens = l.tokenize(sql, sql.len());

                    assert_eq!(
                        l.extract_target_table(&tokens, sql.len())
                            .as_ref()
                            .map(|table| table.table.as_str()),
                        Some("users")
                    );
                    assert_eq!(
                        l.extract_table_references(&tokens)
                            .first()
                            .map(|table| table.table.as_str()),
                        Some("users")
                    );
                }
            }

            #[test]
            fn mysql_upsert_update_is_not_a_table_reference() {
                let l = SqlLexer::new(DatabaseType::MySQL);
                let sql = "INSERT INTO users (id) VALUES (1) ON DUPLICATE KEY UPDATE name = 'Ada'";
                let tokens = l.tokenize(sql, sql.len());

                let references = l.extract_table_references(&tokens);

                assert_eq!(
                    references
                        .iter()
                        .map(|table| table.table.as_str())
                        .collect::<Vec<_>>(),
                    vec!["users"]
                );
            }
        }

        mod locking_clauses {
            use super::*;

            #[test]
            fn for_update_is_not_target() {
                let l = lexer();
                let sql = "SELECT * FROM users FOR UPDATE";
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, sql.len());

                assert!(target.is_none());
            }

            #[test]
            fn for_update_not_in_references() {
                let l = lexer();
                let sql = "SELECT * FROM users FOR UPDATE";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                // Only "users" should be included, FOR UPDATE should not add a reference
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, "users");
            }

            #[test]
            fn for_no_key_update_not_in_references() {
                let l = lexer();
                let sql = "SELECT * FROM users FOR NO KEY UPDATE";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, "users");
            }

            #[test]
            fn for_no_key_update_is_not_target() {
                let l = lexer();
                let sql = "SELECT * FROM users FOR NO KEY UPDATE";
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, sql.len());

                assert!(target.is_none());
            }

            #[test]
            fn multi_statement_for_share_then_update_extracts_both_tables() {
                let l = lexer();
                let sql = "SELECT * FROM users FOR SHARE; UPDATE orders SET status = 'done'";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 2);
                assert_eq!(refs[0].table, "users");
                assert_eq!(refs[1].table, "orders");
            }

            #[test]
            fn multi_statement_for_update_then_update_extracts_both_tables() {
                let l = lexer();
                let sql = "SELECT * FROM users FOR UPDATE; UPDATE orders SET status = 'done'";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 2);
                assert_eq!(refs[0].table, "users");
                assert_eq!(refs[1].table, "orders");
            }

            #[test]
            fn multi_statement_for_no_key_update_then_update_extracts_both_tables() {
                let l = lexer();
                let sql =
                    "SELECT * FROM users FOR NO KEY UPDATE; UPDATE orders SET status = 'done'";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 2);
                assert_eq!(refs[0].table, "users");
                assert_eq!(refs[1].table, "orders");
            }
        }

        mod table_reference_edge_cases {
            use super::*;

            #[test]
            fn with_clause_update_extracts_target() {
                let l = lexer();
                let sql = "WITH active AS (SELECT id FROM users WHERE active) UPDATE users SET status = 'inactive'";
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, sql.len());

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "users");
            }

            #[test]
            fn select_into_not_in_references() {
                let l = lexer();
                let sql = "SELECT * INTO new_table FROM users";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                // Only "users" should be included, not "new_table"
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, "users");
            }

            #[test]
            fn update_only_skips_only_keyword() {
                let l = lexer();
                let sql = "UPDATE ONLY users SET name = 'foo'";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, "users");
            }

            #[test]
            fn update_only_target_table() {
                let l = lexer();
                let sql = "UPDATE ONLY users SET name = 'foo'";
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, sql.len());

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "users");
            }

            #[test]
            fn delete_from_only_skips_only_keyword() {
                let l = lexer();
                let sql = "DELETE FROM ONLY orders WHERE id = 1";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, "orders");
            }

            #[test]
            fn delete_from_only_target_table() {
                let l = lexer();
                let sql = "DELETE FROM ONLY orders WHERE id = 1";
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, sql.len());

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "orders");
            }

            #[test]
            fn insert_into_only_skips_only_keyword() {
                let l = lexer();
                let sql = "INSERT INTO ONLY posts (title) VALUES ('test')";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, "posts");
            }

            #[test]
            fn insert_into_only_target_table() {
                let l = lexer();
                let sql = "INSERT INTO ONLY posts (title) VALUES ('test')";
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, sql.len());

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "posts");
            }

            #[test]
            fn select_from_only_skips_only_keyword() {
                let l = lexer();
                let sql = "SELECT * FROM ONLY users WHERE active = true";
                let tokens = l.tokenize(sql, sql.len());

                let refs = l.extract_table_references(&tokens);

                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].table, "users");
            }
        }

        mod multi_statement_cursor {
            use super::*;

            #[test]
            fn cursor_in_first_update() {
                let l = lexer();
                let sql = "UPDATE users SET x = 1; UPDATE orders SET y = 2";
                // Cursor at position 10 (in "users")
                let cursor_pos = 10;
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "users");
            }

            #[test]
            fn cursor_in_second_update() {
                let l = lexer();
                let sql = "UPDATE users SET x = 1; UPDATE orders SET y = 2";
                // Cursor at position 35 (in "orders")
                let cursor_pos = 35;
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "orders");
            }

            #[test]
            fn cursor_immediately_after_semicolon_uses_next_statement() {
                let l = lexer();
                let sql = "UPDATE users SET x = 1; UPDATE orders SET y = 2";
                let cursor_pos = sql.find(';').unwrap() + 1;
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "orders");
            }

            #[test]
            fn cursor_at_end_of_second_update() {
                let l = lexer();
                let sql = "UPDATE users SET x = 1; UPDATE orders SET y = 2";
                // Cursor at end of SQL
                let cursor_pos = sql.len();
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "orders");
            }

            #[test]
            fn select_then_update_cursor_in_select() {
                let l = lexer();
                let sql = "SELECT * FROM users; UPDATE orders SET status = 'done'";
                // Cursor at position 10 (in SELECT statement)
                let cursor_pos = 10;
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                // SELECT has no target table
                assert!(target.is_none());
            }

            #[test]
            fn select_then_update_cursor_in_update() {
                let l = lexer();
                let sql = "SELECT * FROM users; UPDATE orders SET status = 'done'";
                // Cursor at position 30 (in UPDATE statement)
                let cursor_pos = 30;
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "orders");
            }

            #[test]
            fn three_statements_cursor_in_middle() {
                let l = lexer();
                let sql = "UPDATE users SET x = 1; DELETE FROM posts WHERE id = 1; INSERT INTO orders (status) VALUES ('new')";
                // Cursor at position 40 (in DELETE statement)
                let cursor_pos = 40;
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "posts");
            }

            #[test]
            fn three_statements_cursor_in_last() {
                let l = lexer();
                let sql = "UPDATE users SET x = 1; DELETE FROM posts WHERE id = 1; INSERT INTO orders (status) VALUES ('new')";
                // Cursor at position 80 (in INSERT statement)
                let cursor_pos = 80;
                let tokens = l.tokenize(sql, sql.len());

                let target = l.extract_target_table(&tokens, cursor_pos);

                assert!(target.is_some());
                assert_eq!(target.unwrap().table, "orders");
            }
        }
    }
}
