use super::MysqlLexError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TokenKind {
    Word(String),
    Identifier(String),
    StringLiteral,
    Number,
    Symbol(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) depth: usize,
    pub(super) text: String,
}

pub(super) fn lex_mysql_statement(sql: &str) -> Result<Vec<Token>, MysqlLexError> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;
    let mut leading_executable_version_comment = false;

    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if is_line_comment_start(bytes, index) {
            index = skip_line_comment(bytes, index);
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            let end = skip_block_comment(bytes, index)?;
            let body = &sql[index + 2..end - 2];
            if let Some(body) = body.strip_prefix('!') {
                let body = body.trim_start();
                let version_len = body.bytes().take_while(u8::is_ascii_digit).count();
                if version_len == 0 {
                    return Err(MysqlLexError(
                        "MySQL version comment has no verifiable version".to_string(),
                    ));
                }
                let content = body[version_len..].trim_start();
                if content.is_empty() {
                    return Err(MysqlLexError(
                        "MySQL version comment has no executable statement".to_string(),
                    ));
                }
                let inner = lex_mysql_statement(content)?;
                let executable_statement = inner.first().is_some_and(|token| {
                    matches!(&token.kind, TokenKind::Word(word) if is_mysql_statement_keyword(word))
                });
                let contains_statement_separator = inner
                    .iter()
                    .any(|token| matches!(token.kind, TokenKind::Symbol(';')));
                let trailing_ddl_clause = is_safe_trailing_ddl_clause(&tokens, &inner)
                    && bytes[end..].iter().all(u8::is_ascii_whitespace)
                    && !contains_statement_separator
                    && !inner.iter().any(|token| {
                        matches!(
                            &token.kind,
                            TokenKind::Word(word) if word == "UNSUPPORTED_VERSION_COMMENT"
                        )
                    });
                if tokens.is_empty() && executable_statement && !contains_statement_separator {
                    tokens.extend(inner);
                    leading_executable_version_comment = true;
                } else if trailing_ddl_clause && !executable_statement {
                    // MySQL uses trailing executable comments for versioned DDL clauses such
                    // as DEFAULT CHARSET. Keep the clause out of statement classification.
                } else if !inner.is_empty() {
                    tokens.push(Token {
                        kind: TokenKind::Word("UNSUPPORTED_VERSION_COMMENT".to_string()),
                        depth,
                        text: "UNSUPPORTED_VERSION_COMMENT".to_string(),
                    });
                }
            }
            index = end;
            continue;
        }

        if leading_executable_version_comment {
            tokens.push(Token {
                kind: TokenKind::Word("UNSUPPORTED_VERSION_COMMENT".to_string()),
                depth,
                text: "UNSUPPORTED_VERSION_COMMENT".to_string(),
            });
            leading_executable_version_comment = false;
        }

        let byte = bytes[index];
        if matches!(byte, b'\'' | b'"' | b'`') {
            let end = skip_quoted(bytes, index, byte)?;
            let text = &sql[index + 1..end - 1];
            let kind = if byte == b'`' {
                TokenKind::Identifier(text.replace("``", "`"))
            } else {
                TokenKind::StringLiteral
            };
            tokens.push(Token {
                kind,
                depth,
                text: text.to_string(),
            });
            index = end;
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
            {
                index += 1;
            }
            let text = sql[start..index].to_string();
            tokens.push(Token {
                kind: TokenKind::Word(text.to_ascii_uppercase()),
                depth,
                text,
            });
            continue;
        }
        if byte.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'.' | b'_'))
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                depth,
                text: sql[start..index].to_string(),
            });
            continue;
        }
        let kind = TokenKind::Symbol(byte as char);
        tokens.push(Token {
            kind,
            depth,
            text: (byte as char).to_string(),
        });
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| MysqlLexError("unbalanced MySQL parentheses".to_string()))?;
            }
            _ => {}
        }
        index += 1;
    }
    Ok(tokens)
}

pub(super) fn split_mysql_statements(sql: &str) -> Result<Vec<String>, MysqlLexError> {
    let mut statements = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut quote = None;
    let bytes = sql.as_bytes();
    let mut depth = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(delimiter) = quote {
            if byte == b'\\' && delimiter != b'`' {
                index += 2;
                continue;
            }
            if byte == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                } else {
                    quote = None;
                    index += 1;
                }
                continue;
            }
            index += 1;
            continue;
        }

        if is_line_comment_start(bytes, index) {
            index = skip_line_comment(bytes, index);
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            index = skip_block_comment(bytes, index)?;
            continue;
        }
        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            index += 1;
            continue;
        }
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| MysqlLexError("unbalanced MySQL parentheses".to_string()))?;
            }
            b';' if depth == 0 => {
                push_mysql_statement(&mut statements, &sql[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }

    if quote.is_some() {
        return Err(MysqlLexError(
            "unterminated MySQL quoted literal".to_string(),
        ));
    }
    if depth != 0 {
        return Err(MysqlLexError("unbalanced MySQL parentheses".to_string()));
    }
    push_mysql_statement(&mut statements, &sql[start..]);
    Ok(statements)
}

fn push_mysql_statement(statements: &mut Vec<String>, fragment: &str) {
    let trimmed = fragment.trim();
    if !trimmed.is_empty() && !is_comment_only(trimmed) {
        statements.push(trimmed.to_string());
    }
}

pub(super) fn is_line_comment_start(bytes: &[u8], index: usize) -> bool {
    bytes[index] == b'#'
        || (bytes.get(index..index + 2) == Some(b"--")
            && bytes.get(index + 2).is_none_or(u8::is_ascii_whitespace))
}

pub(super) fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte == b'\n' {
            break;
        }
    }
    index
}

pub(super) fn skip_block_comment(bytes: &[u8], index: usize) -> Result<usize, MysqlLexError> {
    let mut cursor = index + 2;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] == b'*' && bytes[cursor + 1] == b'/' {
            return Ok(cursor + 2);
        }
        cursor += 1;
    }
    Err(MysqlLexError(
        "unterminated MySQL block comment".to_string(),
    ))
}

fn is_safe_trailing_ddl_clause(tokens: &[Token], inner: &[Token]) -> bool {
    let is_create_or_alter = tokens.first().is_some_and(|token: &Token| {
        matches!(
            &token.kind,
            TokenKind::Word(word) if matches!(word.as_str(), "CREATE" | "ALTER")
        )
    });
    let has_safe_clause_start = inner.first().is_some_and(|token| {
        matches!(
            &token.kind,
            TokenKind::Word(word)
                if matches!(
                    word.as_str(),
                    "AUTO_INCREMENT"
                        | "CHARACTER"
                        | "CHARSET"
                        | "COLLATE"
                        | "COMMENT"
                        | "COMPRESSION"
                        | "DEFAULT"
                        | "DEFINER"
                        | "ENCRYPTION"
                        | "ENGINE"
                        | "KEY_BLOCK_SIZE"
                        | "PARTITION"
                        | "ROW_FORMAT"
                        | "SECONDARY_ENGINE"
                        | "SQL"
                        | "STATS_AUTO_RECALC"
                        | "STATS_PERSISTENT"
                        | "STATS_SAMPLE_PAGES"
                        | "TABLESPACE"
                )
        )
    });
    is_create_or_alter
        && has_safe_clause_start
        && !inner.iter().any(|token| {
            matches!(&token.kind, TokenKind::Word(word) if is_mysql_statement_keyword(word))
        })
}

fn is_comment_only(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if is_line_comment_start(bytes, index) {
            index = skip_line_comment(bytes, index);
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            if bytes.get(index + 2) == Some(&b'!') {
                return false;
            }
            index = match skip_block_comment(bytes, index) {
                Ok(next) => next,
                Err(_) => return false,
            };
        } else {
            return false;
        }
    }
    true
}

fn is_mysql_statement_keyword(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "TABLE"
            | "SHOW"
            | "DESCRIBE"
            | "DESC"
            | "WITH"
            | "INSERT"
            | "UPDATE"
            | "DELETE"
            | "CREATE"
            | "ALTER"
            | "RENAME"
            | "DROP"
            | "TRUNCATE"
            | "BEGIN"
            | "START"
            | "COMMIT"
            | "ROLLBACK"
            | "SAVEPOINT"
            | "RELEASE"
            | "USE"
            | "SET"
            | "LOCK"
            | "UNLOCK"
            | "REPLACE"
            | "CALL"
            | "DO"
            | "HANDLER"
            | "LOAD"
            | "PREPARE"
            | "EXECUTE"
            | "DEALLOCATE"
            | "XA"
    )
}

fn skip_quoted(bytes: &[u8], index: usize, quote: u8) -> Result<usize, MysqlLexError> {
    let mut cursor = index + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' && quote != b'`' {
            cursor += 2;
            continue;
        }
        if bytes[cursor] == quote {
            if bytes.get(cursor + 1) == Some(&quote) {
                cursor += 2;
            } else {
                return Ok(cursor + 1);
            }
        } else {
            cursor += 1;
        }
    }
    Err(MysqlLexError(
        "unterminated MySQL quoted literal".to_string(),
    ))
}
