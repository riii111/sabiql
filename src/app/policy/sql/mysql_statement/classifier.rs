use super::{
    MysqlLexError, MysqlStatementKind,
    lexer::{Token, TokenKind},
    target, transaction,
};

type MysqlClassification = (MysqlStatementKind, Option<String>, Option<String>);

pub(super) fn kind_and_target(tokens: &[Token]) -> Result<MysqlClassification, MysqlLexError> {
    let start = target::effective_start(tokens);
    let first = target::word(tokens, start)
        .ok_or_else(|| MysqlLexError("unknown MySQL statement".to_string()))?;
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
        _ => Err(MysqlLexError(format!(
            "unsupported MySQL statement: {first}"
        ))),
    }
}

fn classify_mysql_crud_statement(
    tokens: &[Token],
    start: usize,
    first: &str,
) -> Result<MysqlClassification, MysqlLexError> {
    match first {
        "SELECT" => Ok((MysqlStatementKind::Select, None, None)),
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
                MysqlStatementKind::Replace
            } else {
                MysqlStatementKind::Insert
            };
            Ok((kind, target, database))
        }
        "UPDATE" => {
            let target_index =
                target::skip_mysql_modifiers(tokens, start + 1, &["LOW_PRIORITY", "IGNORE"]);
            let set_index = target::find_word(tokens, "SET", target_index)
                .ok_or_else(|| MysqlLexError("MySQL UPDATE target is ambiguous".to_string()))?;
            if has_multi_table_reference(tokens, target_index, set_index) {
                return Err(MysqlLexError(
                    "MySQL multiple-table UPDATE statements are not supported".to_string(),
                ));
            }
            let has_where = target::top_level_word(&tokens[start..], "WHERE");
            let (target, database) = target::target_after(tokens, target_index)?;
            Ok((MysqlStatementKind::Update { has_where }, target, database))
        }
        "DELETE" => {
            let has_where = target::top_level_word(&tokens[start..], "WHERE");
            let index = target::skip_mysql_modifiers(
                tokens,
                start + 1,
                &["LOW_PRIORITY", "QUICK", "IGNORE"],
            );
            if target::word(tokens, index) != Some("FROM") {
                return Err(MysqlLexError(
                    "MySQL multiple-table DELETE statements are not supported".to_string(),
                ));
            }
            let target_index = index + 1;
            let where_index =
                target::find_word(tokens, "WHERE", target_index).unwrap_or(tokens.len());
            if has_multi_table_reference(tokens, target_index, where_index)
                || has_top_level_word(tokens, "USING", target_index, where_index)
            {
                return Err(MysqlLexError(
                    "MySQL multiple-table DELETE statements are not supported".to_string(),
                ));
            }
            let (target, database) = target::target_after(tokens, target_index)?;
            Ok((MysqlStatementKind::Delete { has_where }, target, database))
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

fn classify_mysql_ddl_statement(
    tokens: &[Token],
    start: usize,
    first: &str,
) -> Result<MysqlClassification, MysqlLexError> {
    match first {
        "CREATE" => {
            let mut index = start + 1;
            if target::word(tokens, index) == Some("OR")
                && target::word(tokens, index + 1) == Some("REPLACE")
                && target::word(tokens, index + 2) == Some("VIEW")
            {
                index += 2;
            }
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
                        MysqlStatementKind::CreateTable { temporary },
                        target,
                        database,
                    ))
                }
                Some("VIEW") => {
                    let (target, database) = target::target_after(tokens, index + 1)?;
                    Ok((MysqlStatementKind::CreateView, target, database))
                }
                Some("FULLTEXT") if target::word(tokens, index + 1) == Some("INDEX") => {
                    let (index_name, _, index_end) = target::identifier_at(tokens, index + 2)
                        .ok_or_else(|| {
                            MysqlLexError("CREATE INDEX name is ambiguous".to_string())
                        })?;
                    if target::word(tokens, index_end) != Some("ON") {
                        return Err(MysqlLexError(
                            "CREATE INDEX target is ambiguous".to_string(),
                        ));
                    }
                    let (_, database) = target::target_after(tokens, index_end + 1)?;
                    Ok((MysqlStatementKind::CreateIndex, Some(index_name), database))
                }
                Some("UNIQUE") if target::word(tokens, index + 1) == Some("INDEX") => {
                    let on = target::find_word(tokens, "ON", index + 2).ok_or_else(|| {
                        MysqlLexError("CREATE INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target::target_after(tokens, on + 1)?;
                    let (index_name, _, _) =
                        target::identifier_at(tokens, index + 2).ok_or_else(|| {
                            MysqlLexError("CREATE INDEX name is ambiguous".to_string())
                        })?;
                    Ok((MysqlStatementKind::CreateIndex, Some(index_name), database))
                }
                Some("INDEX" | "KEY") => {
                    let on = target::find_word(tokens, "ON", index + 1).ok_or_else(|| {
                        MysqlLexError("CREATE INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target::target_after(tokens, on + 1)?;
                    let (index_name, _, _) =
                        target::identifier_at(tokens, index + 1).ok_or_else(|| {
                            MysqlLexError("CREATE INDEX name is ambiguous".to_string())
                        })?;
                    Ok((MysqlStatementKind::CreateIndex, Some(index_name), database))
                }
                _ => Err(MysqlLexError(
                    "unsupported MySQL CREATE statement".to_string(),
                )),
            }
        }
        "ALTER" => match target::word(tokens, start + 1) {
            Some("TABLE") => {
                let (target, database) = target::target_after(tokens, start + 2)?;
                Ok((MysqlStatementKind::AlterTable, target, database))
            }
            Some("VIEW") => {
                let (target, database) = target::target_after(tokens, start + 2)?;
                Ok((MysqlStatementKind::AlterView, target, database))
            }
            _ => Err(MysqlLexError(
                "unsupported MySQL ALTER statement".to_string(),
            )),
        },
        "RENAME" => {
            if target::word(tokens, start + 1) != Some("TABLE") {
                return Err(MysqlLexError(
                    "unsupported MySQL RENAME statement".to_string(),
                ));
            }
            let (target, database, next) = target::identifier_at(tokens, start + 2)
                .ok_or_else(|| MysqlLexError("RENAME TABLE source is ambiguous".to_string()))?;
            if target::word(tokens, next) != Some("TO") {
                return Err(MysqlLexError(
                    "RENAME TABLE requires one source and destination".to_string(),
                ));
            }
            let (_, destination_database, end) = target::identifier_at(tokens, next + 1)
                .ok_or_else(|| {
                    MysqlLexError("RENAME TABLE destination is ambiguous".to_string())
                })?;
            if destination_database.is_some()
                && (database.is_none() || database.as_deref() != destination_database.as_deref())
            {
                return Err(MysqlLexError(
                    "RENAME TABLE cannot move a table across databases".to_string(),
                ));
            }
            if tokens[end..]
                .iter()
                .any(|token| !matches!(token.kind, TokenKind::Symbol(';')))
            {
                return Err(MysqlLexError(
                    "MySQL RENAME TABLE statements must have one source and destination"
                        .to_string(),
                ));
            }
            Ok((MysqlStatementKind::RenameTable, Some(target), database))
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
                        MysqlStatementKind::DropTable { temporary },
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
                    Ok((MysqlStatementKind::DropView, target, database))
                }
                Some("INDEX" | "KEY") => {
                    let on = target::find_word(tokens, "ON", index + 1).ok_or_else(|| {
                        MysqlLexError("DROP INDEX target is ambiguous".to_string())
                    })?;
                    let (_, database) = target::target_after(tokens, on + 1)?;
                    let index_name_index = if target::word(tokens, index + 1) == Some("IF") {
                        index + 3
                    } else {
                        index + 1
                    };
                    let (index_name, _, _) = target::identifier_at(tokens, index_name_index)
                        .ok_or_else(|| MysqlLexError("DROP INDEX name is ambiguous".to_string()))?;
                    Ok((MysqlStatementKind::DropIndex, Some(index_name), database))
                }
                _ => Err(MysqlLexError(
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
            Ok((MysqlStatementKind::TruncateTable, target, database))
        }
        _ => unreachable!("not a MySQL DDL statement: {first}"),
    }
}

fn classify_mysql_utility_statement(first: &str) -> MysqlClassification {
    match first {
        "TABLE" => (MysqlStatementKind::Table, None, None),
        "SHOW" => (MysqlStatementKind::Show, None, None),
        "DESCRIBE" | "DESC" => (MysqlStatementKind::Describe, None, None),
        _ => unreachable!("not a MySQL utility statement: {first}"),
    }
}
