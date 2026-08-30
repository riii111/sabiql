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
    let first = identifier_text(first_token)?;
    if matches!(
        tokens.get(index + 1).map(|token| &token.kind),
        Some(TokenKind::Symbol('.'))
    ) {
        let second_token = tokens.get(index + 2)?;
        let second = identifier_text(second_token)?;
        let next = index + 3;
        if has_unexpected_identifier_boundary(tokens, next) {
            return None;
        }
        return Some((second, Some(first), next));
    }
    let next = index + 1;
    if has_unexpected_identifier_boundary(tokens, next) {
        return None;
    }
    Some((first, None, next))
}

fn has_unexpected_identifier_boundary(tokens: &[Token], index: usize) -> bool {
    tokens.get(index).is_some_and(|token| match &token.kind {
        TokenKind::Symbol(symbol) => !matches!(*symbol, '(' | ';'),
        _ => false,
    })
}

fn identifier_text(token: &Token) -> Option<String> {
    match &token.kind {
        TokenKind::Word(_) => Some(token.text.clone()),
        TokenKind::Identifier(identifier)
            if identifier
                .chars()
                .all(|character| character != '\0' && character <= '\u{FFFF}') =>
        {
            Some(identifier.clone())
        }
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    mod validation {
        use crate::mysql_sql::{
            classify_mysql_multi_statement,
            classify_mysql_multi_statement_with_lower_case_table_names,
        };

        #[test]
        fn ddl_rejects_a_different_database() {
            for sql in [
                "ALTER TABLE other.items ADD COLUMN value INT",
                "ALTER TABLE app.items RENAME TO other.archived_items",
                "ALTER TABLE items RENAME TO other.archived_items",
                "ALTER TABLE app.items RENAME AS other.archived_items",
                "RENAME TABLE app.items TO other.archived_items",
                "DROP TABLE app.keep, other.drop_me",
            ] {
                assert!(
                    classify_mysql_multi_statement(sql, Some("app")).is_err(),
                    "{sql}"
                );
            }
            for sql in [
                "ALTER TABLE app.items ADD COLUMN value INT",
                "RENAME TABLE items TO app.archived_items",
                "ALTER TABLE app.items RENAME TO app.archived_items",
                "ALTER TABLE app.items RENAME AS app.archived_items",
            ] {
                assert!(
                    classify_mysql_multi_statement(sql, Some("app")).is_ok(),
                    "{sql}"
                );
            }
            assert!(classify_mysql_multi_statement("CREATE TABLE items (id INT)", None).is_err());
        }

        #[test]
        fn qualified_mysql_mutations_require_exact_selected_database() {
            for sql in [
                "INSERT INTO other.items VALUES (1)",
                "UPDATE other.items SET value = 1",
                "DELETE FROM other.items WHERE id = 1",
                "INSERT INTO APP.items VALUES (1)",
                "UPDATE APP.items SET value = 1",
                "DELETE FROM APP.items WHERE id = 1",
            ] {
                assert!(
                    classify_mysql_multi_statement(sql, Some("app")).is_err(),
                    "{sql}"
                );
            }

            for sql in [
                "INSERT INTO items VALUES (1)",
                "UPDATE items SET value = 1",
                "DELETE FROM items WHERE id = 1",
                "INSERT INTO app.items VALUES (1)",
                "UPDATE app.items SET value = 1",
                "DELETE FROM app.items WHERE id = 1",
            ] {
                assert!(
                    classify_mysql_multi_statement(sql, Some("app")).is_ok(),
                    "{sql}"
                );
            }
        }

        #[test]
        fn qualified_mysql_mutations_follow_lower_case_table_names() {
            for lower_case_table_names in [1, 2] {
                for sql in [
                    "INSERT INTO APP.items VALUES (1)",
                    "UPDATE APP.items SET value = 1",
                    "DELETE FROM APP.items WHERE id = 1",
                ] {
                    let statements = classify_mysql_multi_statement_with_lower_case_table_names(
                        sql,
                        Some("app"),
                        lower_case_table_names,
                    )
                    .unwrap_or_else(|error| panic!("{sql}: {error}"));
                    assert_eq!(statements[0].target_database.as_deref(), Some("APP"));
                    assert_eq!(statements[0].target(), Some("items"));
                }
            }

            assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "UPDATE APP.items SET value = 1",
                    Some("app"),
                    0,
                )
                .is_err()
            );
            assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "UPDATE other.items SET value = 1",
                    Some("app"),
                    1,
                )
                .is_err()
            );
        }

        #[test]
        fn qualified_utf8_mysql_mutations_follow_selected_database_case_rules() {
            let exact = classify_mysql_multi_statement_with_lower_case_table_names(
                "UPDATE äpp.éléments SET value = 1",
                Some("äpp"),
                0,
            )
            .unwrap();
            assert_eq!(exact[0].target_database.as_deref(), Some("äpp"));
            assert_eq!(exact[0].target(), Some("éléments"));

            for lower_case_table_names in [1, 2] {
                let statements = classify_mysql_multi_statement_with_lower_case_table_names(
                    "UPDATE ÄPP.éléments SET value = 1",
                    Some("äpp"),
                    lower_case_table_names,
                )
                .unwrap();
                assert_eq!(statements[0].target_database.as_deref(), Some("ÄPP"));
                assert_eq!(statements[0].target(), Some("éléments"));
            }

            assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "UPDATE ÄPP.éléments SET value = 1",
                    Some("äpp"),
                    0,
                )
                .is_err()
            );
        }

        #[test]
        fn temporary_mysql_table_keys_follow_unicode_table_case_rules() {
            assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "CREATE TEMPORARY TABLE café (id INT); DROP TEMPORARY TABLE CAFÉ",
                    Some("app"),
                    0,
                )
                .is_err()
            );

            for lower_case_table_names in [1, 2] {
                assert!(
                    classify_mysql_multi_statement_with_lower_case_table_names(
                        "CREATE TEMPORARY TABLE café (id INT); DROP TEMPORARY TABLE CAFÉ",
                        Some("app"),
                        lower_case_table_names,
                    )
                    .is_ok()
                );
            }

            assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "CREATE TEMPORARY TABLE items (id INT); CREATE TEMPORARY TABLE ITEMS (id INT); DROP TEMPORARY TABLE items; DROP TEMPORARY TABLE ITEMS",
                Some("app"),
                0,
            )
            .is_ok()
        );
        }

        #[test]
        fn qualified_mysql_mutations_match_unicode_database_names_without_rewriting_targets() {
            for lower_case_table_names in [1, 2] {
                let statements = classify_mysql_multi_statement_with_lower_case_table_names(
                    "UPDATE `ÄPP`.items SET value = 1",
                    Some("äpp"),
                    lower_case_table_names,
                )
                .unwrap();

                assert_eq!(statements[0].target_database.as_deref(), Some("ÄPP"));
                assert_eq!(statements[0].target(), Some("items"));
            }

            assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "UPDATE `ÄPP`.items SET value = 1",
                    Some("äpp"),
                    0,
                )
                .is_err()
            );
        }

        #[test]
        fn temporary_mysql_table_keys_follow_unicode_database_case_rules() {
            for lower_case_table_names in [1, 2] {
                assert!(
                classify_mysql_multi_statement_with_lower_case_table_names(
                    "CREATE TEMPORARY TABLE temp_items (id INT); DROP TEMPORARY TABLE `ÄPP`.temp_items",
                    Some("äpp"),
                    lower_case_table_names,
                )
                .is_ok()
            );
            }

            assert!(
            classify_mysql_multi_statement_with_lower_case_table_names(
                "CREATE TEMPORARY TABLE temp_items (id INT); DROP TEMPORARY TABLE `ÄPP`.temp_items",
                Some("äpp"),
                0,
            )
            .is_err()
        );
        }

        #[test]
        fn rename_rejects_database_names_that_differ_only_by_case() {
            for sql in [
                "ALTER TABLE APP.items ADD COLUMN value INT",
                "ALTER TABLE app.items RENAME TO APP.archived_items",
                "RENAME TABLE app.items TO APP.archived_items",
            ] {
                assert!(
                    classify_mysql_multi_statement(sql, Some("app")).is_err(),
                    "{sql}"
                );
            }
        }
    }
}
