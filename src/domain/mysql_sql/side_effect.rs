use super::{MySqlLexError, lexer, lexer::TokenKind, target};

pub(super) fn has_top_level_into_clause(sql: &str) -> Result<bool, MySqlLexError> {
    let tokens = lexer::lex_mysql_statement(sql)?;
    Ok(tokens.iter().any(|token| {
        token.depth == 0 && matches!(&token.kind, TokenKind::Word(word) if word == "INTO")
    }))
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
