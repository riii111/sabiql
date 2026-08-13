use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MysqlStatementKind {
    Select,
    Table,
    Show,
    Describe,
    Insert,
    Update { has_where: bool },
    Delete { has_where: bool },
    CreateTable { temporary: bool },
    AlterTable,
    DropTable { temporary: bool },
    TruncateTable,
    CreateView,
    DropView,
    CreateIndex,
    DropIndex,
    Begin,
    StartTransaction,
    Commit,
    Rollback,
    Savepoint,
    RollbackToSavepoint,
    ReleaseSavepoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MysqlStatement {
    pub sql: String,
    pub kind: MysqlStatementKind,
    pub target: Option<String>,
    pub target_database: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MysqlLexError(pub String);

impl fmt::Display for MysqlLexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Word(String),
    Identifier(String),
    StringLiteral,
    Number,
    Symbol(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    depth: usize,
}

pub fn split_mysql_statements(sql: &str) -> Result<Vec<String>, MysqlLexError> {
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

fn is_line_comment_start(bytes: &[u8], index: usize) -> bool {
    bytes[index] == b'#'
        || (bytes.get(index..index + 2) == Some(b"--")
            && bytes.get(index + 2).is_none_or(u8::is_ascii_whitespace))
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte == b'\n' {
            break;
        }
    }
    index
}

fn skip_block_comment(bytes: &[u8], index: usize) -> Result<usize, MysqlLexError> {
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

fn lex_mysql_statement(sql: &str) -> Result<Vec<Token>, MysqlLexError> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;

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
                if tokens.is_empty() && executable_statement {
                    tokens.extend(inner);
                } else if !inner.is_empty() {
                    tokens.push(Token {
                        kind: TokenKind::Word("UNSUPPORTED_VERSION_COMMENT".to_string()),
                        depth,
                    });
                }
            }
            index = end;
            continue;
        }

        let byte = bytes[index];
        if matches!(byte, b'\'' | b'"' | b'`') {
            let end = skip_quoted(bytes, index, byte)?;
            let text = &sql[index + 1..end - 1];
            let kind = if byte == b'`' {
                TokenKind::Identifier(text.replace("``", "`"))
            } else if byte == b'\'' {
                TokenKind::StringLiteral
            } else {
                TokenKind::StringLiteral
            };
            tokens.push(Token { kind, depth });
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
            let word = sql[start..index].to_ascii_uppercase();
            tokens.push(Token {
                kind: TokenKind::Word(word),
                depth,
            });
            continue;
        }
        if byte.is_ascii_digit() {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'.' | b'_'))
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                depth,
            });
            continue;
        }
        let kind = TokenKind::Symbol(byte as char);
        tokens.push(Token { kind, depth });
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

fn word(tokens: &[Token], index: usize) -> Option<&str> {
    match tokens.get(index)?.kind {
        TokenKind::Word(ref word) => Some(word),
        _ => None,
    }
}

fn top_level_word(tokens: &[Token], word_value: &str) -> bool {
    tokens.iter().any(|token| {
        token.depth == 0 && matches!(&token.kind, TokenKind::Word(word) if word == word_value)
    })
}

fn identifier_at(tokens: &[Token], mut index: usize) -> Option<(String, Option<String>, usize)> {
    while matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Symbol(','))
    ) {
        index += 1;
    }
    let first = match &tokens.get(index)?.kind {
        TokenKind::Word(word) => word.clone(),
        TokenKind::Identifier(identifier) => identifier.clone(),
        _ => return None,
    };
    if matches!(
        tokens.get(index + 1).map(|token| &token.kind),
        Some(TokenKind::Symbol('.'))
    ) {
        let second = match &tokens.get(index + 2)?.kind {
            TokenKind::Word(word) => word.clone(),
            TokenKind::Identifier(identifier) => identifier.clone(),
            _ => return None,
        };
        return Some((second, Some(first), index + 3));
    }
    Some((first, None, index + 1))
}

fn skip_mysql_modifiers(tokens: &[Token], mut index: usize, modifiers: &[&str]) -> usize {
    while word(tokens, index).is_some_and(|value| modifiers.contains(&value)) {
        index += 1;
    }
    index
}

fn find_word(tokens: &[Token], value: &str, start: usize) -> Option<usize> {
    tokens
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, token)| {
            (token.depth == 0 && matches!(&token.kind, TokenKind::Word(word) if word == value))
                .then_some(index)
        })
}

fn cte_body_start(tokens: &[Token]) -> Option<usize> {
    if word(tokens, 0) != Some("WITH") {
        return None;
    }
    let mut index = 1;
    if word(tokens, index) == Some("RECURSIVE") {
        index += 1;
    }
    loop {
        if !matches!(
            tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Word(_) | TokenKind::Identifier(_))
        ) {
            return None;
        }
        index += 1;
        if matches!(
            tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Symbol('('))
        ) {
            index = skip_parenthesized_tokens(tokens, index)?;
        }
        if word(tokens, index) != Some("AS") {
            return None;
        }
        index += 1;
        if !matches!(
            tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Symbol('('))
        ) {
            return None;
        }
        index = skip_parenthesized_tokens(tokens, index)?;
        if matches!(
            tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Symbol(','))
        ) {
            index += 1;
            continue;
        }
        return Some(index);
    }
}

fn skip_parenthesized_tokens(tokens: &[Token], index: usize) -> Option<usize> {
    if !matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Symbol('('))
    ) {
        return None;
    }
    let mut depth = 0usize;
    for cursor in index..tokens.len() {
        match tokens[cursor].kind {
            TokenKind::Symbol('(') => depth += 1,
            TokenKind::Symbol(')') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn drop_target_after(
    tokens: &[Token],
    index: usize,
) -> Result<(Option<String>, Option<String>), MysqlLexError> {
    let (target, database, next) = identifier_at(tokens, index)
        .ok_or_else(|| MysqlLexError("MySQL statement target is ambiguous".to_string()))?;
    if tokens[next..]
        .iter()
        .any(|token| token.depth == 0 && matches!(token.kind, TokenKind::Symbol(',')))
    {
        return Err(MysqlLexError(
            "MySQL DROP statements must have one target".to_string(),
        ));
    }
    Ok((Some(target), database))
}

fn effective_start(tokens: &[Token]) -> usize {
    cte_body_start(tokens).unwrap_or(0)
}

fn kind_and_target(
    tokens: &[Token],
) -> Result<(MysqlStatementKind, Option<String>, Option<String>), MysqlLexError> {
    let start = effective_start(tokens);
    let first =
        word(tokens, start).ok_or_else(|| MysqlLexError("unknown MySQL statement".to_string()))?;
    let target_after = |index: usize| {
        identifier_at(tokens, index)
            .map(|(target, database, _)| (Some(target), database))
            .ok_or_else(|| MysqlLexError("MySQL statement target is ambiguous".to_string()))
    };

    let result = match first {
        "SELECT" => (MysqlStatementKind::Select, None, None),
        "TABLE" => (MysqlStatementKind::Table, None, None),
        "SHOW" => (MysqlStatementKind::Show, None, None),
        "DESCRIBE" | "DESC" => (MysqlStatementKind::Describe, None, None),
        "INSERT" => {
            let index = skip_mysql_modifiers(
                tokens,
                start + 1,
                &["LOW_PRIORITY", "DELAYED", "HIGH_PRIORITY", "IGNORE"],
            );
            let target_index = if word(tokens, index) == Some("INTO") {
                index + 1
            } else {
                index
            };
            let (target, database) = target_after(target_index)?;
            (MysqlStatementKind::Insert, target, database)
        }
        "UPDATE" => {
            let has_where = top_level_word(&tokens[start..], "WHERE");
            let target_index = skip_mysql_modifiers(tokens, start + 1, &["LOW_PRIORITY", "IGNORE"]);
            let (target, database) = target_after(target_index)?;
            (MysqlStatementKind::Update { has_where }, target, database)
        }
        "DELETE" => {
            let has_where = top_level_word(&tokens[start..], "WHERE");
            let index =
                skip_mysql_modifiers(tokens, start + 1, &["LOW_PRIORITY", "QUICK", "IGNORE"]);
            let target_index = if word(tokens, index) == Some("FROM") {
                index + 1
            } else {
                index
            };
            let (target, database) = target_after(target_index)?;
            (MysqlStatementKind::Delete { has_where }, target, database)
        }
        "CREATE" => {
            let mut index = start + 1;
            let temporary = word(tokens, index) == Some("TEMPORARY");
            if temporary {
                index += 1;
            }
            match word(tokens, index) {
                Some("TABLE") => {
                    index += 1;
                    if word(tokens, index) == Some("IF") {
                        index += 3;
                    }
                    let (target, database) = target_after(index)?;
                    (
                        MysqlStatementKind::CreateTable { temporary },
                        target,
                        database,
                    )
                }
                Some("VIEW") => {
                    let (target, database) = target_after(index + 1)?;
                    (MysqlStatementKind::CreateView, target, database)
                }
                Some("UNIQUE") if word(tokens, index + 1) == Some("INDEX") => {
                    let on = find_word(tokens, "ON", index + 2).ok_or_else(|| {
                        MysqlLexError("CREATE INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target_after(on + 1)?;
                    let (index_name, _, _) = identifier_at(tokens, index + 2).ok_or_else(|| {
                        MysqlLexError("CREATE INDEX name is ambiguous".to_string())
                    })?;
                    (MysqlStatementKind::CreateIndex, Some(index_name), database)
                }
                Some("INDEX" | "KEY") => {
                    let on = find_word(tokens, "ON", index + 1).ok_or_else(|| {
                        MysqlLexError("CREATE INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target_after(on + 1)?;
                    let (index_name, _, _) = identifier_at(tokens, index + 1).ok_or_else(|| {
                        MysqlLexError("CREATE INDEX name is ambiguous".to_string())
                    })?;
                    (MysqlStatementKind::CreateIndex, Some(index_name), database)
                }
                _ => {
                    return Err(MysqlLexError(
                        "unsupported MySQL CREATE statement".to_string(),
                    ));
                }
            }
        }
        "ALTER" => {
            if word(tokens, start + 1) != Some("TABLE") {
                return Err(MysqlLexError(
                    "unsupported MySQL ALTER statement".to_string(),
                ));
            }
            let (target, database) = target_after(start + 2)?;
            (MysqlStatementKind::AlterTable, target, database)
        }
        "DROP" => {
            let mut index = start + 1;
            let temporary = word(tokens, index) == Some("TEMPORARY");
            if temporary {
                index += 1;
            }
            match word(tokens, index) {
                Some("TABLE") => {
                    let target_index = if word(tokens, index + 1) == Some("IF") {
                        index + 3
                    } else {
                        index + 1
                    };
                    let (target, database) = drop_target_after(tokens, target_index)?;
                    (
                        MysqlStatementKind::DropTable { temporary },
                        target,
                        database,
                    )
                }
                Some("VIEW") => {
                    let target_index = if word(tokens, index + 1) == Some("IF") {
                        index + 3
                    } else {
                        index + 1
                    };
                    let (target, database) = drop_target_after(tokens, target_index)?;
                    (MysqlStatementKind::DropView, target, database)
                }
                Some("INDEX" | "KEY") => {
                    let on = find_word(tokens, "ON", index + 1).ok_or_else(|| {
                        MysqlLexError("DROP INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target_after(on + 1)?;
                    let index_name_index = if word(tokens, index + 1) == Some("IF") {
                        index + 3
                    } else {
                        index + 1
                    };
                    let (index_name, _, _) = identifier_at(tokens, index_name_index)
                        .ok_or_else(|| MysqlLexError("DROP INDEX name is ambiguous".to_string()))?;
                    (MysqlStatementKind::DropIndex, Some(index_name), database)
                }
                _ => {
                    return Err(MysqlLexError(
                        "unsupported MySQL DROP statement".to_string(),
                    ));
                }
            }
        }
        "TRUNCATE" => {
            let target_index = if word(tokens, start + 1) == Some("TABLE") {
                start + 2
            } else {
                start + 1
            };
            let (target, database) = target_after(target_index)?;
            (MysqlStatementKind::TruncateTable, target, database)
        }
        "BEGIN" => {
            if tokens.len() != start + 1 {
                return Err(MysqlLexError(
                    "BEGIN modifiers are not supported".to_string(),
                ));
            }
            (MysqlStatementKind::Begin, None, None)
        }
        "START" if word(tokens, start + 1) == Some("TRANSACTION") => {
            if tokens.len() != start + 2 {
                return Err(MysqlLexError(
                    "START TRANSACTION modifiers are not supported".to_string(),
                ));
            }
            (MysqlStatementKind::StartTransaction, None, None)
        }
        "COMMIT" => {
            if tokens.len() != start + 1 {
                return Err(MysqlLexError(
                    "COMMIT modifiers are not supported".to_string(),
                ));
            }
            (MysqlStatementKind::Commit, None, None)
        }
        "ROLLBACK" => {
            if word(tokens, start + 1).is_none() && tokens.len() == start + 1 {
                (MysqlStatementKind::Rollback, None, None)
            } else if word(tokens, start + 1) == Some("TO") {
                let name_index = if word(tokens, start + 2) == Some("SAVEPOINT") {
                    start + 3
                } else {
                    start + 2
                };
                if tokens.len() != name_index + 1 {
                    return Err(MysqlLexError(
                        "ROLLBACK modifiers are not supported".to_string(),
                    ));
                }
                let (name, database, _) = identifier_at(tokens, name_index).ok_or_else(|| {
                    MysqlLexError("ROLLBACK SAVEPOINT name is ambiguous".to_string())
                })?;
                (
                    MysqlStatementKind::RollbackToSavepoint,
                    Some(name),
                    database,
                )
            } else {
                return Err(MysqlLexError(
                    "ROLLBACK modifiers are not supported".to_string(),
                ));
            }
        }
        "SAVEPOINT" if tokens.len() == start + 2 => {
            let (name, database, _) = identifier_at(tokens, start + 1)
                .ok_or_else(|| MysqlLexError("SAVEPOINT name is ambiguous".to_string()))?;
            (MysqlStatementKind::Savepoint, Some(name), database)
        }
        "RELEASE" if word(tokens, start + 1) == Some("SAVEPOINT") && tokens.len() == start + 3 => {
            let (name, database, _) = identifier_at(tokens, start + 2)
                .ok_or_else(|| MysqlLexError("RELEASE SAVEPOINT name is ambiguous".to_string()))?;
            (MysqlStatementKind::ReleaseSavepoint, Some(name), database)
        }
        _ => {
            return Err(MysqlLexError(format!(
                "unsupported MySQL statement: {first}"
            )));
        }
    };
    Ok(result)
}

pub fn classify_mysql_statement(sql: &str) -> Result<MysqlStatement, MysqlLexError> {
    let tokens = lex_mysql_statement(sql)?;
    if tokens.is_empty() {
        return Err(MysqlLexError("empty MySQL statement".to_string()));
    }
    if tokens.iter().any(|token| {
        matches!(
            &token.kind,
            TokenKind::Word(word) if word == "UNSUPPORTED_VERSION_COMMENT"
        )
    }) {
        return Err(MysqlLexError(
            "executable MySQL version comment contains another statement".to_string(),
        ));
    }
    let (kind, target, target_database) = kind_and_target(&tokens)?;
    Ok(MysqlStatement {
        sql: sql.to_string(),
        kind,
        target,
        target_database,
    })
}

pub fn has_top_level_into_clause(sql: &str) -> Result<bool, MysqlLexError> {
    let tokens = lex_mysql_statement(sql)?;
    Ok(tokens.iter().any(|token| {
        token.depth == 0 && matches!(&token.kind, TokenKind::Word(word) if word == "INTO")
    }))
}

pub fn target_is_selected_database(
    statement: &MysqlStatement,
    selected_database: Option<&str>,
) -> bool {
    match (statement.target_database.as_deref(), selected_database) {
        (Some(target_database), Some(selected)) => target_database.eq_ignore_ascii_case(selected),
        (None, Some(_)) => true,
        (Some(_) | None, None) => false,
    }
}

pub fn statement_contains_unsupported_mysql_control(sql: &str) -> bool {
    let lower = sql.trim_start().to_ascii_lowercase();
    [
        "use ",
        "use\n",
        "set ",
        "set\n",
        "lock tables",
        "unlock tables",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_mysql_comments_quotes_and_backticks() {
        let sql = "SELECT 'a;\\'b'; # comment;\n SELECT `semi;colon` /* ; */";
        assert_eq!(split_mysql_statements(sql).unwrap().len(), 2);
    }

    #[test]
    fn rejects_ambiguous_quotes() {
        assert!(split_mysql_statements("SELECT 'unfinished").is_err());
    }

    #[test]
    fn classifies_with_and_version_comment() {
        let statement = classify_mysql_statement(
            "WITH rows(id) AS (SELECT 1) UPDATE `app`.`items` SET value = 1",
        )
        .unwrap();
        assert!(matches!(statement.kind, MysqlStatementKind::Update { .. }));
        assert_eq!(statement.target, Some("items".to_string()));
    }

    #[test]
    fn rejects_unverifiable_version_comment() {
        assert!(classify_mysql_statement("/*! SET sql_mode='ANSI_QUOTES' */ SELECT 1").is_err());
    }

    #[test]
    fn classifies_leading_version_comment_statement() {
        assert!(matches!(
            classify_mysql_statement("/*!80000 SELECT 1 */"),
            Ok(MysqlStatement {
                kind: MysqlStatementKind::Select,
                ..
            })
        ));
    }

    #[test]
    fn rejects_executable_version_comment_clause() {
        assert!(classify_mysql_statement("SELECT 1 /*!80000 INTO OUTFILE '/tmp/x' */").is_err());
    }

    #[test]
    fn rejects_multiple_drop_targets_and_ambiguous_ddl_quotes() {
        assert!(classify_mysql_statement("DROP TABLE app.keep, other.drop_me").is_err());
        assert!(classify_mysql_statement("DROP VIEW app.keep, other.drop_me").is_err());
        assert!(classify_mysql_statement("CREATE TABLE \"items\" (id INT)").is_err());
    }

    #[test]
    fn rejects_executable_inline_control_statement() {
        assert!(
            classify_mysql_statement("SELECT 1 /*!80000 SET sql_mode='ANSI_QUOTES' */").is_err()
        );
    }

    #[test]
    fn keeps_index_confirmation_name_separate_from_ddl_database_target() {
        let statement = classify_mysql_statement("DROP INDEX ix ON app.items").unwrap();
        assert_eq!(statement.target, Some("IX".to_string()));
        assert_eq!(statement.target_database, Some("APP".to_string()));
        let statement = classify_mysql_statement("DROP INDEX IF EXISTS ix ON app.items").unwrap();
        assert_eq!(statement.target, Some("IX".to_string()));
    }
}
