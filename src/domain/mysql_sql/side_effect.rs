use super::{MySqlLexError, lexer, lexer::TokenKind, target};

pub(super) fn has_top_level_into_clause(sql: &str) -> Result<bool, MySqlLexError> {
    let tokens = lexer::lex_mysql_statement(sql)?;
    Ok(tokens.iter().any(|token| {
        token.depth == 0 && matches!(&token.kind, TokenKind::Word(word) if word == "INTO")
    }))
}

pub(super) fn has_top_level_user_variable_into_clause(sql: &str) -> Result<bool, MySqlLexError> {
    let tokens = lexer::lex_mysql_statement(sql)?;
    Ok(tokens.iter().enumerate().any(|(index, token)| {
        token.depth == 0
            && matches!(&token.kind, TokenKind::Word(word) if word == "INTO")
            && is_user_variable_into_clause(&tokens, index)
    }))
}

fn is_user_variable_into_clause(tokens: &[lexer::Token], index: usize) -> bool {
    matches!(
        tokens.get(index + 1).map(|token| &token.kind),
        Some(TokenKind::Symbol('@'))
    ) && tokens.get(index + 2).is_some_and(|token| {
        matches!(
            &token.kind,
            TokenKind::Word(_)
                | TokenKind::Identifier(_)
                | TokenKind::Number
                | TokenKind::StringLiteral
        )
    })
}

pub(super) fn has_mysql_read_only_side_effect(sql: &str) -> Result<bool, MySqlLexError> {
    if has_mysql_version_comment(sql)? {
        return Ok(true);
    }

    let tokens = lexer::lex_mysql_statement(sql)?;
    for (index, token) in tokens.iter().enumerate() {
        if matches!(&token.kind, TokenKind::Word(word) if word == "INTO") {
            return Ok(true);
        }
        if has_mysql_read_only_side_effect_function(&tokens, index) {
            return Ok(true);
        }
        if matches!(&token.kind, TokenKind::Symbol(':'))
            && matches!(
                tokens.get(index + 1).map(|token| &token.kind),
                Some(TokenKind::Symbol('='))
            )
        {
            return Ok(true);
        }
        if target::word(&tokens, index) == Some("FOR")
            && matches!(target::word(&tokens, index + 1), Some("UPDATE" | "SHARE"))
        {
            return Ok(true);
        }
        if target::word(&tokens, index) == Some("LOCK")
            && target::word(&tokens, index + 1) == Some("IN")
            && target::word(&tokens, index + 2) == Some("SHARE")
            && target::word(&tokens, index + 3) == Some("MODE")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn mysql_statement_reads_session_diagnostics(sql: &str) -> Result<bool, MySqlLexError> {
    let tokens = lexer::lex_mysql_statement(sql)?;
    if tokens.windows(2).any(|window| {
        let (TokenKind::Word(function) | TokenKind::Identifier(function)) = &window[0].kind else {
            return false;
        };
        matches!(window[1].kind, TokenKind::Symbol('('))
            && ["FOUND_ROWS", "ROW_COUNT", "LAST_INSERT_ID"]
                .iter()
                .any(|name| function.eq_ignore_ascii_case(name))
    }) {
        return Ok(true);
    }
    Ok((0..tokens.len().saturating_sub(2)).any(|index| {
        if !matches!(tokens[index].kind, TokenKind::Symbol('@'))
            || !matches!(tokens[index + 1].kind, TokenKind::Symbol('@'))
        {
            return false;
        }
        let name_index = if matches!(
            tokens.get(index + 3).map(|token| &token.kind),
            Some(TokenKind::Symbol('.'))
        ) {
            index + 4
        } else {
            index + 2
        };
        tokens.get(name_index).is_some_and(|token| {
            matches!(&token.kind, TokenKind::Word(name) | TokenKind::Identifier(name)
                if ["WARNING_COUNT", "ERROR_COUNT"]
                    .iter()
                    .any(|diagnostic| name.eq_ignore_ascii_case(diagnostic)))
        })
    }))
}

fn has_mysql_read_only_side_effect_function(tokens: &[lexer::Token], index: usize) -> bool {
    let Some(word) = tokens.get(index).and_then(|token| match &token.kind {
        TokenKind::Word(word) | TokenKind::Identifier(word) => Some(word.as_str()),
        _ => None,
    }) else {
        return false;
    };
    if !matches!(
        tokens.get(index + 1).map(|token| &token.kind),
        Some(TokenKind::Symbol('('))
    ) {
        return false;
    }

    if ["GET_LOCK", "RELEASE_LOCK", "RELEASE_ALL_LOCKS"]
        .iter()
        .any(|name| word.eq_ignore_ascii_case(name))
    {
        return true;
    }
    word.eq_ignore_ascii_case("LAST_INSERT_ID")
        && !matches!(
            tokens.get(index + 2).map(|token| &token.kind),
            Some(TokenKind::Symbol(')'))
        )
}

pub(super) fn has_mysql_version_comment(sql: &str) -> Result<bool, MySqlLexError> {
    let bytes = sql.as_bytes();
    let mut index = 0;
    let mut quote = None;

    while index < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' && delimiter != b'`' {
                index = (index + 2).min(bytes.len());
            } else if bytes[index] == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                } else {
                    quote = None;
                    index += 1;
                }
            } else {
                index += 1;
            }
            continue;
        }
        if lexer::is_line_comment_start(bytes, index) {
            index = lexer::skip_line_comment(bytes, index);
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            if bytes.get(index + 2) == Some(&b'!') {
                return Ok(true);
            }
            index = lexer::skip_block_comment(bytes, index)?;
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            quote = Some(bytes[index]);
        }
        index += 1;
    }

    if quote.is_some() {
        return Err(MySqlLexError(
            "unterminated MySQL quoted literal".to_string(),
        ));
    }
    Ok(false)
}

pub(super) fn statement_contains_unsupported_mysql_control(sql: &str) -> bool {
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
        || contains_mysql_client_command(sql)
}

fn contains_mysql_client_command(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    let mut line_start = true;
    let mut quote = None;
    let mut block_comment = false;

    while index < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' && delimiter != b'`' {
                index = (index + 2).min(bytes.len());
            } else if bytes[index] == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                } else {
                    quote = None;
                    index += 1;
                }
            } else {
                index += 1;
            }
            continue;
        }
        if block_comment {
            if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment = false;
                index += 2;
            } else {
                line_start = bytes[index] == b'\n' || line_start;
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'\\' {
            return true;
        }
        if line_start {
            if matches!(bytes[index], b' ' | b'\t' | b'\r') {
                index += 1;
                continue;
            }
            if bytes[index] == b'\n' {
                index += 1;
                continue;
            }
            if bytes[index] == b'\\' {
                return true;
            }
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment = true;
                index += 2;
                continue;
            }
            if lexer::is_line_comment_start(bytes, index) {
                index = lexer::skip_line_comment(bytes, index);
                continue;
            }
            if bytes[index].is_ascii_alphabetic() {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                let word = &sql[start..index];
                if matches!(
                    word.to_ascii_lowercase().as_str(),
                    "delimiter" | "charset" | "source" | "system"
                ) && (index == bytes.len() || bytes[index].is_ascii_whitespace())
                {
                    return true;
                }
                line_start = false;
                continue;
            }
            line_start = false;
        }

        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment = true;
            index += 2;
        } else if lexer::is_line_comment_start(bytes, index) {
            index = lexer::skip_line_comment(bytes, index);
            line_start = true;
        } else if bytes[index] == b'\n' {
            line_start = true;
            index += 1;
        } else if matches!(bytes[index], b'\'' | b'"' | b'`') {
            quote = Some(bytes[index]);
            line_start = false;
            index += 1;
        } else {
            index += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_word_and_quoted_mysql_side_effect_functions() {
        for function in ["GET_LOCK", "RELEASE_LOCK", "RELEASE_ALL_LOCKS"] {
            for quoted in [false, true] {
                let name = if quoted {
                    format!("`{function}`")
                } else {
                    function.to_string()
                };
                let sql = match function {
                    "GET_LOCK" => format!("SELECT {name}('sabiql', 0)"),
                    "RELEASE_LOCK" => format!("SELECT {name}('sabiql')"),
                    "RELEASE_ALL_LOCKS" => format!("SELECT {name}()"),
                    _ => unreachable!(),
                };
                assert!(has_mysql_read_only_side_effect(&sql).unwrap(), "{sql}");
            }
        }
    }

    #[test]
    fn rejects_mixed_case_quoted_last_insert_id_with_an_argument() {
        assert!(has_mysql_read_only_side_effect("SELECT `Last_Insert_Id`(42)").unwrap());
        assert!(!has_mysql_read_only_side_effect("SELECT `Last_Insert_Id`()").unwrap());
    }

    #[test]
    fn detects_mysql_session_diagnostic_reads() {
        for function in ["FOUND_ROWS", "ROW_COUNT", "LAST_INSERT_ID"] {
            assert!(
                mysql_statement_reads_session_diagnostics(&format!("SELECT {function}()")).unwrap()
            );
        }
        for variable in ["warning_count", "error_count"] {
            assert!(
                mysql_statement_reads_session_diagnostics(&format!("SELECT @@{variable}")).unwrap()
            );
            assert!(
                mysql_statement_reads_session_diagnostics(&format!("SELECT @@SESSION.{variable}"))
                    .unwrap()
            );
        }
        assert!(!mysql_statement_reads_session_diagnostics("SELECT 'FOUND_ROWS()'").unwrap());
    }

    #[test]
    fn detects_top_level_into_clauses_and_user_variable_shape() {
        for sql in [
            "SELECT id INTO @picked FROM items",
            "SELECT id INTO @picked, @other FROM items",
        ] {
            assert!(has_top_level_into_clause(sql).unwrap(), "{sql}");
            assert!(
                has_top_level_user_variable_into_clause(sql).unwrap(),
                "{sql}"
            );
        }
        for sql in [
            "SELECT id INTO OUTFILE '/tmp/result' FROM items",
            "SELECT id INTO DUMPFILE '/tmp/result' FROM items",
        ] {
            assert!(has_top_level_into_clause(sql).unwrap(), "{sql}");
            assert!(
                !has_top_level_user_variable_into_clause(sql).unwrap(),
                "{sql}"
            );
        }
    }

    #[test]
    fn ignores_quoted_commented_and_nested_into_tokens() {
        for sql in [
            "SELECT 'INTO @picked' FROM items",
            "SELECT /* INTO OUTFILE '/tmp/result' */ id FROM items",
            "WITH rows AS (SELECT id INTO @picked FROM items) SELECT * FROM rows",
        ] {
            assert!(!has_top_level_into_clause(sql).unwrap(), "{sql}");
            assert!(
                !has_top_level_user_variable_into_clause(sql).unwrap(),
                "{sql}"
            );
        }
    }
}
