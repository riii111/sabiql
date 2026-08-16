use super::{MySqlLexError, MySqlStatementKind, lexer::Token, target};

pub(super) fn classify_mysql_transaction_statement(
    tokens: &[Token],
    start: usize,
    first: &str,
) -> Result<(MySqlStatementKind, Option<String>, Option<String>), MySqlLexError> {
    match first {
        "BEGIN" => {
            if tokens.len() != start + 1 {
                return Err(MySqlLexError(
                    "BEGIN modifiers are not supported".to_string(),
                ));
            }
            Ok((MySqlStatementKind::Begin, None, None))
        }
        "START" if target::word(tokens, start + 1) == Some("TRANSACTION") => {
            if tokens.len() != start + 2 {
                return Err(MySqlLexError(
                    "START TRANSACTION modifiers are not supported".to_string(),
                ));
            }
            Ok((MySqlStatementKind::StartTransaction, None, None))
        }
        "COMMIT" => {
            if tokens.len() != start + 1 {
                return Err(MySqlLexError(
                    "COMMIT modifiers are not supported".to_string(),
                ));
            }
            Ok((MySqlStatementKind::Commit, None, None))
        }
        "ROLLBACK" => {
            if target::word(tokens, start + 1).is_none() && tokens.len() == start + 1 {
                Ok((MySqlStatementKind::Rollback, None, None))
            } else if target::word(tokens, start + 1) == Some("TO") {
                let name_index = if target::word(tokens, start + 2) == Some("SAVEPOINT") {
                    start + 3
                } else {
                    start + 2
                };
                if tokens.len() != name_index + 1 {
                    return Err(MySqlLexError(
                        "ROLLBACK modifiers are not supported".to_string(),
                    ));
                }
                let (name, database, _) =
                    target::identifier_at(tokens, name_index).ok_or_else(|| {
                        MySqlLexError("ROLLBACK SAVEPOINT name is ambiguous".to_string())
                    })?;
                Ok((
                    MySqlStatementKind::RollbackToSavepoint,
                    Some(name),
                    database,
                ))
            } else {
                Err(MySqlLexError(
                    "ROLLBACK modifiers are not supported".to_string(),
                ))
            }
        }
        "SAVEPOINT" if tokens.len() == start + 2 => {
            let (name, database, _) = target::identifier_at(tokens, start + 1)
                .ok_or_else(|| MySqlLexError("SAVEPOINT name is ambiguous".to_string()))?;
            Ok((MySqlStatementKind::Savepoint, Some(name), database))
        }
        "RELEASE"
            if target::word(tokens, start + 1) == Some("SAVEPOINT")
                && tokens.len() == start + 3 =>
        {
            let (name, database, _) = target::identifier_at(tokens, start + 2)
                .ok_or_else(|| MySqlLexError("RELEASE SAVEPOINT name is ambiguous".to_string()))?;
            Ok((MySqlStatementKind::ReleaseSavepoint, Some(name), database))
        }
        _ => Err(MySqlLexError(format!(
            "unsupported MySQL statement: {first}"
        ))),
    }
}
