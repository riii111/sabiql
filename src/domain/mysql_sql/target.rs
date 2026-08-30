use super::{
    MySqlLexError, MySqlStatement,
    lexer::{Token, TokenKind},
};

pub(super) fn target_after(
    tokens: &[Token],
    index: usize,
) -> Result<(Option<String>, Option<String>), MySqlLexError> {
    identifier_at(tokens, index)
        .map(|(target, database, _)| (Some(target), database))
        .ok_or_else(|| MySqlLexError("MySQL statement target is ambiguous".to_string()))
}

pub(super) fn drop_target_after(
    tokens: &[Token],
    index: usize,
) -> Result<(Option<String>, Option<String>), MySqlLexError> {
    let (target, database, next) = identifier_at(tokens, index)
        .ok_or_else(|| MySqlLexError("MySQL statement target is ambiguous".to_string()))?;
    if tokens[next..]
        .iter()
        .any(|token| token.depth == 0 && matches!(token.kind, TokenKind::Symbol(',')))
    {
        return Err(MySqlLexError(
            "MySQL DROP statements must have one target".to_string(),
        ));
    }
    Ok((Some(target), database))
}

pub(super) fn effective_start(tokens: &[Token]) -> usize {
    cte_body_start(tokens).unwrap_or(0)
}

pub(super) fn target_is_selected_database_with_lower_case_table_names(
    statement: &MySqlStatement,
    selected_database: Option<&str>,
    lower_case_table_names: u8,
) -> bool {
    match (statement.target_database.as_deref(), selected_database) {
        (Some(target_database), Some(selected)) => {
            database_names_match(target_database, selected, lower_case_table_names)
        }
        (None, Some(_)) => true,
        (Some(_) | None, None) => false,
    }
}

fn database_names_match(left: &str, right: &str, lower_case_table_names: u8) -> bool {
    match lower_case_table_names {
        0 => left == right,
        1 | 2 => left.to_lowercase() == right.to_lowercase(),
        _ => false,
    }
}

pub(super) fn word(tokens: &[Token], index: usize) -> Option<&str> {
    match tokens.get(index)?.kind {
        TokenKind::Word(ref word) => Some(word),
        _ => None,
    }
}

pub(super) fn top_level_word(tokens: &[Token], word_value: &str) -> bool {
    tokens.iter().any(|token| {
        token.depth == 0 && matches!(&token.kind, TokenKind::Word(word) if word == word_value)
    })
}

pub(super) fn identifier_at(
    tokens: &[Token],
    mut index: usize,
) -> Option<(String, Option<String>, usize)> {
    while matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Symbol(','))
    ) {
        index += 1;
    }
    let first_token = tokens.get(index)?;
    let first = match &first_token.kind {
        TokenKind::Word(_) => first_token.text.clone(),
        TokenKind::Identifier(identifier) => identifier.clone(),
        _ => return None,
    };
    if matches!(
        tokens.get(index + 1).map(|token| &token.kind),
        Some(TokenKind::Symbol('.'))
    ) {
        let second_token = tokens.get(index + 2)?;
        let second = match &second_token.kind {
            TokenKind::Word(_) => second_token.text.clone(),
            TokenKind::Identifier(identifier) => identifier.clone(),
            _ => return None,
        };
        return Some((second, Some(first), index + 3));
    }
    Some((first, None, index + 1))
}

pub(super) fn skip_mysql_modifiers(
    tokens: &[Token],
    mut index: usize,
    modifiers: &[&str],
) -> usize {
    while word(tokens, index).is_some_and(|value| modifiers.contains(&value)) {
        index += 1;
    }
    index
}

pub(super) fn find_word(tokens: &[Token], value: &str, start: usize) -> Option<usize> {
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
    for (cursor, token) in tokens.iter().enumerate().skip(index) {
        match token.kind {
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
