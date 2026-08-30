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

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn unicode_savepoints_are_case_insensitive_in_transactions() {
        assert!(
            classify_mysql_multi_statement(
                "START TRANSACTION; SAVEPOINT café; ROLLBACK TO SAVEPOINT CAFÉ; COMMIT",
                Some("app"),
            )
            .is_ok()
        );
        assert!(
            classify_mysql_multi_statement(
                "START TRANSACTION; SAVEPOINT ς; ROLLBACK TO SAVEPOINT σ; COMMIT",
                Some("app"),
            )
            .is_ok()
        );
    }

    #[test]
    fn modifiers_are_rejected() {
        for sql in [
            "START TRANSACTION READ ONLY",
            "COMMIT AND CHAIN",
            "ROLLBACK AND NO CHAIN",
            "ROLLBACK TO SAVEPOINT named extra",
            "RELEASE SAVEPOINT named extra",
            "BEGIN",
            "BEGIN; UPDATE items SET value = 1",
            "SAVEPOINT named",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_err(),
                "{sql}"
            );
        }
        for sql in [
            "COMMIT",
            "ROLLBACK",
            "BEGIN; COMMIT",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; ROLLBACK",
            "START TRANSACTION; ROLLBACK",
            "BEGIN; SAVEPOINT named; ROLLBACK TO named; RELEASE SAVEPOINT named; COMMIT",
            "CREATE TEMPORARY TABLE temp_items (id INT); INSERT INTO temp_items VALUES (1); SELECT * FROM temp_items",
            "CREATE TEMPORARY TABLE temp_items (id INT); DROP TEMPORARY TABLE app.temp_items",
        ] {
            assert!(
                classify_mysql_multi_statement(sql, Some("app")).is_ok(),
                "{sql}"
            );
        }
    }

    #[test]
    fn rejects_persistent_ddl_inside_an_explicit_transaction() {
        for sql in [
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; CREATE TABLE new_items (id INT); ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; ALTER TABLE items ADD COLUMN extra INT; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; DROP TABLE items; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; TRUNCATE TABLE items; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; CREATE VIEW item_view AS SELECT 1; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; DROP VIEW item_view; ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; CREATE INDEX item_index ON items (value); ROLLBACK",
            "BEGIN; UPDATE items SET value = 1 WHERE id = 1; DROP INDEX item_index ON items; ROLLBACK",
        ] {
            let error = classify_mysql_multi_statement(sql, Some("app")).unwrap_err();
            assert!(error.contains("implicit commit"), "{sql}: {error}");
        }
    }
}
