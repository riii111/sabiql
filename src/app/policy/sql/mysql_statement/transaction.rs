use super::{MysqlLexError, MysqlStatementKind, lexer::Token, target};

pub(super) fn classify_mysql_transaction_statement(
    tokens: &[Token],
    start: usize,
    first: &str,
) -> Result<(MysqlStatementKind, Option<String>, Option<String>), MysqlLexError> {
    match first {
        "BEGIN" => {
            if tokens.len() != start + 1 {
                return Err(MysqlLexError(
                    "BEGIN modifiers are not supported".to_string(),
                ));
            }
            Ok((MysqlStatementKind::Begin, None, None))
        }
        "START" if target::word(tokens, start + 1) == Some("TRANSACTION") => {
            if tokens.len() != start + 2 {
                return Err(MysqlLexError(
                    "START TRANSACTION modifiers are not supported".to_string(),
                ));
            }
            Ok((MysqlStatementKind::StartTransaction, None, None))
        }
        "COMMIT" => {
            if tokens.len() != start + 1 {
                return Err(MysqlLexError(
                    "COMMIT modifiers are not supported".to_string(),
                ));
            }
            Ok((MysqlStatementKind::Commit, None, None))
        }
        "ROLLBACK" => {
            if target::word(tokens, start + 1).is_none() && tokens.len() == start + 1 {
                Ok((MysqlStatementKind::Rollback, None, None))
            } else if target::word(tokens, start + 1) == Some("TO") {
                let name_index = if target::word(tokens, start + 2) == Some("SAVEPOINT") {
                    start + 3
                } else {
                    start + 2
                };
                if tokens.len() != name_index + 1 {
                    return Err(MysqlLexError(
                        "ROLLBACK modifiers are not supported".to_string(),
                    ));
                }
                let (name, database, _) =
                    target::identifier_at(tokens, name_index).ok_or_else(|| {
                        MysqlLexError("ROLLBACK SAVEPOINT name is ambiguous".to_string())
                    })?;
                Ok((
                    MysqlStatementKind::RollbackToSavepoint,
                    Some(name),
                    database,
                ))
            } else {
                Err(MysqlLexError(
                    "ROLLBACK modifiers are not supported".to_string(),
                ))
            }
        }
        "SAVEPOINT" if tokens.len() == start + 2 => {
            let (name, database, _) = target::identifier_at(tokens, start + 1)
                .ok_or_else(|| MysqlLexError("SAVEPOINT name is ambiguous".to_string()))?;
            Ok((MysqlStatementKind::Savepoint, Some(name), database))
        }
        "RELEASE"
            if target::word(tokens, start + 1) == Some("SAVEPOINT")
                && tokens.len() == start + 3 =>
        {
            let (name, database, _) = target::identifier_at(tokens, start + 2)
                .ok_or_else(|| MysqlLexError("RELEASE SAVEPOINT name is ambiguous".to_string()))?;
            Ok((MysqlStatementKind::ReleaseSavepoint, Some(name), database))
        }
        _ => Err(MysqlLexError(format!(
            "unsupported MySQL statement: {first}"
        ))),
    }
}
