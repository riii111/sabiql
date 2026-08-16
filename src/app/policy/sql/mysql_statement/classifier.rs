use super::{
    MySqlLexError, MySqlStatementKind,
    lexer::{Token, TokenKind},
    target, transaction,
};

type MySqlClassification = (MySqlStatementKind, Option<String>, Option<String>);

pub(super) fn kind_and_target(tokens: &[Token]) -> Result<MySqlClassification, MySqlLexError> {
    let start = target::effective_start(tokens);
    let first = target::word(tokens, start)
        .ok_or_else(|| MySqlLexError("unknown MySQL statement".to_string()))?;
    match first {
        "SELECT" | "INSERT" | "REPLACE" | "UPDATE" | "DELETE" => {
            classify_mysql_crud_statement(tokens, start, first)
        }
        "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "RENAME" => {
            classify_mysql_ddl_statement(tokens, start, first)
        }
        "BEGIN" | "START" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE" => {
            transaction::classify_mysql_transaction_statement(tokens, start, first)
        }
        "TABLE" | "SHOW" | "DESCRIBE" | "DESC" => Ok(classify_mysql_utility_statement(first)),
        _ => Err(MySqlLexError(format!(
            "unsupported MySQL statement: {first}"
        ))),
    }
}

fn classify_mysql_crud_statement(
    tokens: &[Token],
    start: usize,
    first: &str,
) -> Result<MySqlClassification, MySqlLexError> {
    match first {
        "SELECT" => Ok((MySqlStatementKind::Select, None, None)),
        "INSERT" | "REPLACE" => {
            let index = if first == "INSERT" {
                target::skip_mysql_modifiers(
                    tokens,
                    start + 1,
                    &["LOW_PRIORITY", "DELAYED", "HIGH_PRIORITY", "IGNORE"],
                )
            } else {
                target::skip_mysql_modifiers(tokens, start + 1, &["LOW_PRIORITY", "DELAYED"])
            };
            let target_index = if target::word(tokens, index) == Some("INTO") {
                index + 1
            } else {
                index
            };
            let (target, database) = target::target_after(tokens, target_index)?;
            let kind = if first == "REPLACE" {
                MySqlStatementKind::Replace
            } else {
                MySqlStatementKind::Insert
            };
            Ok((kind, target, database))
        }
        "UPDATE" => {
            let target_index =
                target::skip_mysql_modifiers(tokens, start + 1, &["LOW_PRIORITY", "IGNORE"]);
            let set_index = target::find_word(tokens, "SET", target_index)
                .ok_or_else(|| MySqlLexError("MySQL UPDATE target is ambiguous".to_string()))?;
            if has_multi_table_reference(tokens, target_index, set_index) {
                return Err(MySqlLexError(
                    "MySQL multiple-table UPDATE statements are not supported".to_string(),
                ));
            }
            let has_where = target::top_level_word(&tokens[start..], "WHERE");
            let (target, database) = target::target_after(tokens, target_index)?;
            Ok((MySqlStatementKind::Update { has_where }, target, database))
        }
        "DELETE" => {
            let has_where = target::top_level_word(&tokens[start..], "WHERE");
            let index = target::skip_mysql_modifiers(
                tokens,
                start + 1,
                &["LOW_PRIORITY", "QUICK", "IGNORE"],
            );
            if target::word(tokens, index) != Some("FROM") {
                return Err(MySqlLexError(
                    "MySQL multiple-table DELETE statements are not supported".to_string(),
                ));
            }
            let target_index = index + 1;
            let where_index =
                target::find_word(tokens, "WHERE", target_index).unwrap_or(tokens.len());
            if has_multi_table_reference(tokens, target_index, where_index)
                || has_top_level_word(tokens, "USING", target_index, where_index)
            {
                return Err(MySqlLexError(
                    "MySQL multiple-table DELETE statements are not supported".to_string(),
                ));
            }
            let (target, database) = target::target_after(tokens, target_index)?;
            Ok((MySqlStatementKind::Delete { has_where }, target, database))
        }
        _ => unreachable!("not a MySQL CRUD statement: {first}"),
    }
}

fn has_multi_table_reference(tokens: &[Token], start: usize, end: usize) -> bool {
    tokens
        .iter()
        .enumerate()
        .skip(start)
        .take(end.saturating_sub(start))
        .any(|(index, token)| {
            token.depth == 0
                && match &token.kind {
                    TokenKind::Symbol(',') => true,
                    TokenKind::Word(word) => {
                        matches!(word.as_str(), "JOIN" | "STRAIGHT_JOIN")
                            && !is_mysql_index_hint_join(tokens, index)
                    }
                    _ => false,
                }
        })
}

fn is_mysql_index_hint_join(tokens: &[Token], index: usize) -> bool {
    index >= 2
        && target::word(tokens, index - 1) == Some("FOR")
        && matches!(target::word(tokens, index - 2), Some("INDEX" | "KEY"))
}

fn has_top_level_word(tokens: &[Token], expected: &str, start: usize, end: usize) -> bool {
    tokens
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .any(|token| {
            token.depth == 0 && matches!(&token.kind, TokenKind::Word(word) if word == expected)
        })
}

fn view_statement_start(tokens: &[Token], start: usize, allow_or_replace: bool) -> Option<usize> {
    let mut index = start + 1;
    if allow_or_replace
        && target::word(tokens, index) == Some("OR")
        && target::word(tokens, index + 1) == Some("REPLACE")
    {
        index += 2;
    }
    loop {
        match target::word(tokens, index) {
            Some("VIEW") => return Some(index),
            Some("ALGORITHM") => {
                if !matches!(
                    tokens.get(index + 1).map(|token| &token.kind),
                    Some(TokenKind::Symbol('='))
                ) || !matches!(
                    target::word(tokens, index + 2),
                    Some("UNDEFINED" | "MERGE" | "TEMPTABLE")
                ) {
                    return None;
                }
                index += 3;
            }
            Some("DEFINER") => {
                if !matches!(
                    tokens.get(index + 1).map(|token| &token.kind),
                    Some(TokenKind::Symbol('='))
                ) {
                    return None;
                }
                index = skip_view_definer(tokens, index + 2)?;
            }
            Some("SQL") => {
                if target::word(tokens, index + 1) != Some("SECURITY")
                    || !matches!(target::word(tokens, index + 2), Some("DEFINER" | "INVOKER"))
                {
                    return None;
                }
                index += 3;
            }
            _ => return None,
        }
    }
}

fn skip_view_definer(tokens: &[Token], mut index: usize) -> Option<usize> {
    let is_current_user = target::word(tokens, index) == Some("CURRENT_USER");
    if !matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Word(_) | TokenKind::Identifier(_) | TokenKind::StringLiteral)
    ) {
        return None;
    }
    index += 1;
    if is_current_user
        && matches!(
            tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Symbol('('))
        )
    {
        if !matches!(
            tokens.get(index + 1).map(|token| &token.kind),
            Some(TokenKind::Symbol(')'))
        ) {
            return None;
        }
        index += 2;
    }
    if matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Symbol('@'))
    ) {
        if !matches!(
            tokens.get(index + 1).map(|token| &token.kind),
            Some(TokenKind::Word(_) | TokenKind::Identifier(_) | TokenKind::StringLiteral)
        ) {
            return None;
        }
        index += 2;
    }
    Some(index)
}

fn classify_mysql_ddl_statement(
    tokens: &[Token],
    start: usize,
    first: &str,
) -> Result<MySqlClassification, MySqlLexError> {
    match first {
        "CREATE" => {
            if let Some(view_index) = view_statement_start(tokens, start, true) {
                let (target, database) = target::target_after(tokens, view_index + 1)?;
                return Ok((MySqlStatementKind::CreateView, target, database));
            }
            let mut index = start + 1;
            let temporary = target::word(tokens, index) == Some("TEMPORARY");
            if temporary {
                index += 1;
            }
            match target::word(tokens, index) {
                Some("TABLE") => {
                    index += 1;
                    if target::word(tokens, index) == Some("IF") {
                        index += 3;
                    }
                    let (target, database) = target::target_after(tokens, index)?;
                    Ok((
                        MySqlStatementKind::CreateTable { temporary },
                        target,
                        database,
                    ))
                }
                Some("VIEW") => {
                    let (target, database) = target::target_after(tokens, index + 1)?;
                    Ok((MySqlStatementKind::CreateView, target, database))
                }
                Some("FULLTEXT") if target::word(tokens, index + 1) == Some("INDEX") => {
                    let (index_name, _, index_end) = target::identifier_at(tokens, index + 2)
                        .ok_or_else(|| {
                            MySqlLexError("CREATE INDEX name is ambiguous".to_string())
                        })?;
                    if target::word(tokens, index_end) != Some("ON") {
                        return Err(MySqlLexError(
                            "CREATE INDEX target is ambiguous".to_string(),
                        ));
                    }
                    let (_, database) = target::target_after(tokens, index_end + 1)?;
                    Ok((MySqlStatementKind::CreateIndex, Some(index_name), database))
                }
                Some("UNIQUE") if target::word(tokens, index + 1) == Some("INDEX") => {
                    let on = target::find_word(tokens, "ON", index + 2).ok_or_else(|| {
                        MySqlLexError("CREATE INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target::target_after(tokens, on + 1)?;
                    let (index_name, _, _) =
                        target::identifier_at(tokens, index + 2).ok_or_else(|| {
                            MySqlLexError("CREATE INDEX name is ambiguous".to_string())
                        })?;
                    Ok((MySqlStatementKind::CreateIndex, Some(index_name), database))
                }
                Some("INDEX" | "KEY") => {
                    let on = target::find_word(tokens, "ON", index + 1).ok_or_else(|| {
                        MySqlLexError("CREATE INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target::target_after(tokens, on + 1)?;
                    let (index_name, _, _) =
                        target::identifier_at(tokens, index + 1).ok_or_else(|| {
                            MySqlLexError("CREATE INDEX name is ambiguous".to_string())
                        })?;
                    Ok((MySqlStatementKind::CreateIndex, Some(index_name), database))
                }
                _ => Err(MySqlLexError(
                    "unsupported MySQL CREATE statement".to_string(),
                )),
            }
        }
        "ALTER" => {
            if let Some(view_index) = view_statement_start(tokens, start, false) {
                let (target, database) = target::target_after(tokens, view_index + 1)?;
                return Ok((MySqlStatementKind::AlterView, target, database));
            }
            match target::word(tokens, start + 1) {
                Some("TABLE") => {
                    let (target, database) = classify_alter_table_target(tokens, start + 2)?;
                    Ok((MySqlStatementKind::AlterTable, target, database))
                }
                _ => Err(MySqlLexError(
                    "unsupported MySQL ALTER statement".to_string(),
                )),
            }
        }
        "RENAME" => {
            if target::word(tokens, start + 1) != Some("TABLE") {
                return Err(MySqlLexError(
                    "unsupported MySQL RENAME statement".to_string(),
                ));
            }
            let (target, database, next) = target::identifier_at(tokens, start + 2)
                .ok_or_else(|| MySqlLexError("RENAME TABLE source is ambiguous".to_string()))?;
            if target::word(tokens, next) != Some("TO") {
                return Err(MySqlLexError(
                    "RENAME TABLE requires one source and destination".to_string(),
                ));
            }
            let (_, destination_database, end) = target::identifier_at(tokens, next + 1)
                .ok_or_else(|| {
                    MySqlLexError("RENAME TABLE destination is ambiguous".to_string())
                })?;
            let effective_database = match (database.as_deref(), destination_database.as_deref()) {
                (Some(source), Some(destination)) if source != destination => {
                    return Err(MySqlLexError(
                        "RENAME TABLE cannot move a table across databases".to_string(),
                    ));
                }
                (Some(source), _) => Some(source.to_string()),
                (None, Some(destination)) => Some(destination.to_string()),
                (None, None) => None,
            };
            if tokens[end..]
                .iter()
                .any(|token| !matches!(token.kind, TokenKind::Symbol(';')))
            {
                return Err(MySqlLexError(
                    "MySQL RENAME TABLE statements must have one source and destination"
                        .to_string(),
                ));
            }
            Ok((
                MySqlStatementKind::RenameTable,
                Some(target),
                effective_database,
            ))
        }
        "DROP" => {
            let mut index = start + 1;
            let temporary = target::word(tokens, index) == Some("TEMPORARY");
            if temporary {
                index += 1;
            }
            match target::word(tokens, index) {
                Some("TABLE") => {
                    let target_index = if target::word(tokens, index + 1) == Some("IF") {
                        index + 3
                    } else {
                        index + 1
                    };
                    let (target, database) = target::drop_target_after(tokens, target_index)?;
                    Ok((
                        MySqlStatementKind::DropTable { temporary },
                        target,
                        database,
                    ))
                }
                Some("VIEW") => {
                    let target_index = if target::word(tokens, index + 1) == Some("IF") {
                        index + 3
                    } else {
                        index + 1
                    };
                    let (target, database) = target::drop_target_after(tokens, target_index)?;
                    Ok((MySqlStatementKind::DropView, target, database))
                }
                Some("INDEX" | "KEY") => {
                    let on = target::find_word(tokens, "ON", index + 1).ok_or_else(|| {
                        MySqlLexError("DROP INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target::target_after(tokens, on + 1)?;
                    let index_name_index = if target::word(tokens, index + 1) == Some("IF") {
                        index + 3
                    } else {
                        index + 1
                    };
                    let (index_name, _, _) = target::identifier_at(tokens, index_name_index)
                        .ok_or_else(|| MySqlLexError("DROP INDEX name is ambiguous".to_string()))?;
                    Ok((MySqlStatementKind::DropIndex, Some(index_name), database))
                }
                _ => Err(MySqlLexError(
                    "unsupported MySQL DROP statement".to_string(),
                )),
            }
        }
        "TRUNCATE" => {
            let target_index = if target::word(tokens, start + 1) == Some("TABLE") {
                start + 2
            } else {
                start + 1
            };
            let (target, database) = target::target_after(tokens, target_index)?;
            Ok((MySqlStatementKind::TruncateTable, target, database))
        }
        _ => unreachable!("not a MySQL DDL statement: {first}"),
    }
}

fn classify_alter_table_target(
    tokens: &[Token],
    table_index: usize,
) -> Result<(Option<String>, Option<String>), MySqlLexError> {
    let (target, source_database, after_target) = target::identifier_at(tokens, table_index)
        .ok_or_else(|| MySqlLexError("MySQL statement target is ambiguous".to_string()))?;
    if alter_table_target_is_ambiguous(tokens, table_index) {
        return Err(MySqlLexError(
            "MySQL ALTER TABLE target is ambiguous".to_string(),
        ));
    }
    let destination_database = alter_table_rename_database(tokens, after_target)?;
    let effective_database = match (source_database.as_deref(), destination_database.as_deref()) {
        (Some(source), Some(destination)) if source != destination => {
            return Err(MySqlLexError(
                "ALTER TABLE RENAME cannot move a table across databases".to_string(),
            ));
        }
        (Some(source), _) => Some(source.to_string()),
        (None, Some(destination)) => Some(destination.to_string()),
        (None, None) => None,
    };
    Ok((Some(target), effective_database))
}

fn alter_table_target_is_ambiguous(tokens: &[Token], table_index: usize) -> bool {
    if matches!(
        tokens.get(table_index + 1).map(|token| &token.kind),
        Some(TokenKind::Symbol('='))
    ) {
        return true;
    }

    if target::word(tokens, table_index).is_some_and(is_mysql_reserved_alter_word) {
        return true;
    }

    if matches!(
        target::word(tokens, table_index),
        Some("SECONDARY_LOAD" | "SECONDARY_UNLOAD")
    ) && (matches!(target::word(tokens, table_index + 1), Some("PARTITION"))
        || tokens[table_index + 1..]
            .iter()
            .all(|token| matches!(token.kind, TokenKind::Symbol(';'))))
    {
        return true;
    }

    matches!(
        (
            target::word(tokens, table_index),
            target::word(tokens, table_index + 1)
        ),
        (
            Some(
                "CHECK"
                    | "COALESCE"
                    | "EXCHANGE"
                    | "MODIFY"
                    | "OPTIMIZE"
                    | "REBUILD"
                    | "REORGANIZE"
                    | "REPAIR"
                    | "TRUNCATE"
            ),
            Some("COLUMN" | "PARTITION")
        ) | (Some("DISABLE" | "ENABLE"), Some("KEYS"))
            | (Some("DISCARD" | "IMPORT"), Some("TABLESPACE"))
            | (Some("REMOVE" | "UPGRADE"), Some("PARTITIONING"))
            | (Some("WITH" | "WITHOUT"), Some("VALIDATION"))
    )
}

fn is_mysql_reserved_alter_word(word: &str) -> bool {
    matches!(
        word,
        "ADD"
            | "ALTER"
            | "ANALYZE"
            | "AS"
            | "BY"
            | "CHANGE"
            | "CHECK"
            | "CHARACTER"
            | "COLLATE"
            | "COLUMN"
            | "CONSTRAINT"
            | "CONVERT"
            | "DEFAULT"
            | "DROP"
            | "FORCE"
            | "FOREIGN"
            | "FULLTEXT"
            | "INDEX"
            | "KEY"
            | "LOCK"
            | "ORDER"
            | "PARTITION"
            | "PRIMARY"
            | "RENAME"
            | "TO"
            | "UNION"
            | "UNIQUE"
            | "WITH"
    )
}

fn alter_table_rename_database(
    tokens: &[Token],
    start: usize,
) -> Result<Option<String>, MySqlLexError> {
    let Some(destination_start) =
        tokens
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(index, token)| {
                (token.depth == 0
                    && target::word(tokens, index) == Some("RENAME")
                    && matches!(target::word(tokens, index + 1), Some("TO" | "AS")))
                .then_some(index + 2)
            })
    else {
        return Ok(None);
    };
    let (_, database, _) = target::identifier_at(tokens, destination_start)
        .ok_or_else(|| MySqlLexError("ALTER TABLE RENAME destination is ambiguous".to_string()))?;
    Ok(database)
}

fn classify_mysql_utility_statement(first: &str) -> MySqlClassification {
    match first {
        "TABLE" => (MySqlStatementKind::Table, None, None),
        "SHOW" => (MySqlStatementKind::Show, None, None),
        "DESCRIBE" | "DESC" => (MySqlStatementKind::Describe, None, None),
        _ => unreachable!("not a MySQL utility statement: {first}"),
    }
}
