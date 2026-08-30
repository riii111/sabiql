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

pub(super) fn has_on_duplicate_key_update(tokens: &[Token]) -> bool {
    tokens.windows(4).any(|window| {
        window.iter().all(|token| token.depth == 0)
            && matches!(&window[0].kind, TokenKind::Word(word) if word == "ON")
            && matches!(&window[1].kind, TokenKind::Word(word) if word == "DUPLICATE")
            && matches!(&window[2].kind, TokenKind::Word(word) if word == "KEY")
            && matches!(&window[3].kind, TokenKind::Word(word) if word == "UPDATE")
    })
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
            let table_reference_end = ["WHERE", "ORDER", "LIMIT"]
                .into_iter()
                .filter_map(|word| target::find_word(tokens, word, target_index))
                .min()
                .unwrap_or(tokens.len());
            if has_multi_table_reference(tokens, target_index, table_reference_end)
                || has_top_level_word(tokens, "USING", target_index, table_reference_end)
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

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn splits_mysql_comments_quotes_and_backticks() {
        let sql = "SELECT 'a;\\'b'; # comment;\n SELECT `semi;colon` /* ; */";
        assert_eq!(split_mysql_statements(sql).unwrap().len(), 2);
    }

    #[test]
    fn classified_statement_preserves_its_private_classifier_invariant() {
        let statement = classify_mysql_statement("DROP TABLE users").unwrap();

        assert_eq!(statement.sql(), "DROP TABLE users");
        assert_eq!(
            statement.kind(),
            &MySqlStatementKind::DropTable { temporary: false }
        );
        assert_eq!(statement.target(), Some("users"));
        assert_eq!(statement.target_database.as_deref(), None);
        assert!(validate_mysql_statements(&[statement], Some("app")).is_ok());
    }

    #[test]
    fn classifies_insert_on_duplicate_key_update_shape() {
        let upsert = classify_mysql_statement(
            "INSERT INTO users (id, name) VALUES (1, 'Ada') ON DUPLICATE KEY UPDATE name = 'Grace'",
        )
        .unwrap();
        let insert =
            classify_mysql_statement("INSERT INTO users (id, name) VALUES (1, 'Ada')").unwrap();

        assert!(upsert.has_on_duplicate_key_update());
        assert!(!insert.has_on_duplicate_key_update());
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
        assert!(matches!(statement.kind, MySqlStatementKind::Update { .. }));
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
            Ok(MySqlStatement {
                kind: MySqlStatementKind::Select,
                ..
            })
        ));
    }

    #[test]
    fn preserves_utf8_unquoted_and_backtick_targets() {
        let cases = [
            ("UPDATE café SET value = 1", None, "café"),
            ("UPDATE 1é SET value = 1", None, "1é"),
            ("UPDATE 1abc SET value = 1", None, "1abc"),
            ("UPDATE 1_foo SET value = 1", None, "1_foo"),
            ("UPDATE 1$foo SET value = 1", None, "1$foo"),
            ("UPDATE 1$é SET value = 1", None, "1$é"),
            (
                "UPDATE 1abc.éléments SET value = 1",
                Some("1abc"),
                "éléments",
            ),
            ("UPDATE 1$foo.café SET value = 1", Some("1$foo"), "café"),
            ("UPDATE 1_foo.items SET value = 1", Some("1_foo"), "items"),
            (
                "UPDATE café.éléments SET value = 1",
                Some("café"),
                "éléments",
            ),
            ("UPDATE $items SET value = 1", None, "$items"),
            (
                r"UPDATE `café`.`éléments` SET value = 1",
                Some("café"),
                "éléments",
            ),
        ];

        for (sql, expected_database, expected_target) in cases {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(
                statement.target_database.as_deref(),
                expected_database,
                "{sql}"
            );
            assert_eq!(statement.target(), Some(expected_target), "{sql}");
        }

        assert!(classify_mysql_statement("UPDATE 123 SET value = 1").is_err());
    }

    #[test]
    fn rejects_mysql_numeric_literals_as_mutation_targets() {
        for sql in [
            "UPDATE 1e3 SET value = 1",
            "UPDATE 1e+3 SET value = 1",
            "UPDATE 1e-3 SET value = 1",
            "UPDATE 0x01AF SET value = 1",
            "UPDATE 0b01 SET value = 1",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn preserves_utf8_targets_around_comments_and_executable_comments() {
        for sql in [
            "UPDATE /* ignored café */ café SET value = 1",
            "UPDATE café -- ignored comment\n SET value = 1",
            "/*!80000 UPDATE café SET value = 1 */",
            "CREATE TABLE café (id INT) /*!40100 DEFAULT CHARSET=utf8mb4 */",
            "DROP TABLE café /*!80000 RESTRICT */",
        ] {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(statement.target(), Some("café"), "{sql}");
        }

        for sql in [
            "UPDATE café--x\n SET value = 1",
            "UPDATE 1e+foo SET value = 1",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn rejects_non_bmp_backtick_targets_fail_closed() {
        assert!(classify_mysql_statement("UPDATE `caf😀` SET value = 1").is_err());
        assert!(classify_mysql_statement("UPDATE caf😀 SET value = 1").is_err());
    }

    #[test]
    fn classifies_documented_mysql_ddl_forms() {
        let cases = [
            (
                "RENAME TABLE app.items TO app.archived_items",
                MySqlStatementKind::RenameTable,
                "items",
                Some("app"),
            ),
            (
                "RENAME TABLE items TO app.archived_items",
                MySqlStatementKind::RenameTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME TO app.archived_items",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME AS app.archived_items",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME COLUMN old_name TO new_name",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "ALTER TABLE app.items RENAME INDEX old_index TO new_index",
                MySqlStatementKind::AlterTable,
                "items",
                Some("app"),
            ),
            (
                "CREATE OR REPLACE VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::CreateView,
                "item_view",
                Some("app"),
            ),
            (
                "CREATE OR REPLACE ALGORITHM=MERGE DEFINER=CURRENT_USER SQL SECURITY INVOKER VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::CreateView,
                "item_view",
                Some("app"),
            ),
            (
                "ALTER VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::AlterView,
                "item_view",
                Some("app"),
            ),
            (
                "ALTER ALGORITHM=MERGE DEFINER=CURRENT_USER SQL SECURITY INVOKER VIEW app.item_view AS SELECT id FROM app.items",
                MySqlStatementKind::AlterView,
                "item_view",
                Some("app"),
            ),
            (
                "CREATE FULLTEXT INDEX item_text ON app.items (body)",
                MySqlStatementKind::CreateIndex,
                "item_text",
                Some("app"),
            ),
        ];

        for (sql, expected_kind, expected_target, expected_database) in cases {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(statement.kind, expected_kind, "{sql}");
            assert_eq!(statement.target.as_deref(), Some(expected_target), "{sql}");
            assert_eq!(
                statement.target_database.as_deref(),
                expected_database,
                "{sql}"
            );
        }
    }

    #[test]
    fn accepts_only_trailing_ddl_version_comments() {
        let statement = classify_mysql_statement(
            "CREATE TABLE items (id INT) /*!40100 DEFAULT CHARSET=utf8mb4 */",
        )
        .expect("trailing DDL version comment");
        assert_eq!(
            statement.kind,
            MySqlStatementKind::CreateTable { temporary: false }
        );
        assert!(
            classify_mysql_statement("CREATE TABLE items (id INT) /* ordinary comment */").is_ok()
        );
        assert!(
            classify_mysql_statement(
                "CREATE TABLE items (id INT) /*!401 DEFAULT DROP TABLE other_items */"
            )
            .is_ok()
        );
        assert!(
            classify_mysql_statement(
                "CREATE TABLE items (id INT) /*!080400 DEFAULT CHARSET=utf8mb4 */"
            )
            .is_ok()
        );
        assert!(classify_mysql_statement("DROP TABLE items /*!80000 RESTRICT */").is_ok());
        assert!(classify_mysql_statement("DROP TABLE items /*!80000 CASCADE */").is_ok());

        for sql in [
            "CREATE TABLE items (id INT) /*!40100 DEFAULT CHARSET=utf8mb4 */ SELECT 1",
            "CREATE TABLE items (id INT) /*!40100 SET sql_mode='ANSI_QUOTES' */",
            "CREATE TABLE items (id INT) /*!80000 DEFAULT CHARSET=utf8mb4 DROP TABLE other_items */",
            "DROP TABLE items /*!80000 , other_items */",
            "CREATE TABLE items (id INT) /*!8000011 DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!80000DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!080000DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*! 80000 DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!40100 */",
            "CREATE TABLE items (id INT) /*! DEFAULT CHARSET=utf8mb4 */",
            "CREATE TABLE items (id INT) /*!40100 DEFAULT CHARSET=utf8mb4",
            "SELECT 1 /*!40101 + 1 */",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn rejects_unsupported_or_ambiguous_ddl_forms() {
        for sql in [
            "CREATE DATABASE app",
            "ALTER DATABASE app CHARACTER SET utf8mb4",
            "ALTER TABLE",
            "ALTER TABLE PARTITION BY HASH(id)",
            "ALTER TABLE ORDER BY value",
            "RENAME DATABASE app TO archive",
            "RENAME TABLE old_items TO archived_items, other_items TO other_archive",
            "RENAME TABLE app.items TO other.archived_items",
            "ALTER TABLE app.items RENAME TO other.archived_items",
            "ALTER TABLE app.items RENAME AS other.archived_items",
            "CREATE OR REPLACE TABLE items (id INT)",
            "CREATE OR REPLACE INDEX item_index ON items (id)",
            "CREATE SPATIAL INDEX item_location ON items (location)",
        ] {
            assert!(classify_mysql_statement(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn rejects_executable_version_comment_clause() {
        assert!(classify_mysql_statement("SELECT 1 /*!80000 INTO OUTFILE '/tmp/x' */").is_err());
        assert!(classify_mysql_statement("/*!80000 SELECT 1 */ SELECT 2").is_err());
        assert!(classify_mysql_statement("/*!80000 SELECT 1; DROP TABLE items */").is_err());
    }

    #[test]
    fn accepts_sql_calc_found_rows_in_an_executable_select_modifier_comment() {
        assert!(
            classify_mysql_statement(
                "SELECT /*!80000 SQL_CALC_FOUND_ROWS */ first_key FROM items WHERE FALSE"
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_multiple_drop_targets_and_ambiguous_ddl_quotes() {
        assert!(classify_mysql_statement("DROP TABLE app.keep, other.drop_me").is_err());
        assert!(classify_mysql_statement("DROP VIEW app.keep, other.drop_me").is_err());
        assert!(classify_mysql_statement("CREATE TABLE \"items\" (id INT)").is_err());
    }

    #[test]
    fn rejects_multiple_table_update_and_delete_statements() {
        for sql in [
            "UPDATE items, prices SET items.price = prices.price WHERE items.id = prices.id",
            "UPDATE items JOIN prices ON items.id = prices.id SET items.price = prices.price",
            "DELETE items, prices FROM items JOIN prices ON items.id = prices.id",
            "DELETE FROM items, prices USING items JOIN prices ON items.id = prices.id",
            "DELETE items FROM items JOIN prices ON items.id = prices.id",
            "DELETE FROM items USING items JOIN prices ON items.id = prices.id",
            "DELETE FROM items JOIN prices ON items.id = prices.id",
        ] {
            let error = classify_mysql_statement(sql).unwrap_err();
            assert!(error.0.contains("multiple-table"), "{sql}: {error}");
        }
    }

    #[test]
    fn accepts_single_table_delete_clause_boundaries_and_nested_commas() {
        for sql in [
            "DELETE FROM items ORDER BY created_at, id LIMIT 10",
            "DELETE FROM items WHERE id IN (SELECT item_id FROM prices, currencies)",
            "DELETE FROM items LIMIT 10",
            "DELETE FROM `items,archive` ORDER BY id LIMIT 10",
        ] {
            let statement = classify_mysql_statement(sql).expect(sql);
            assert_eq!(
                statement.kind,
                MySqlStatementKind::Delete {
                    has_where: sql.contains("WHERE")
                },
                "{sql}"
            );
        }
    }

    #[test]
    fn rejects_top_level_into_clauses_before_execution() {
        for sql in [
            "SELECT id INTO OUTFILE '/tmp/result' FROM items",
            "SELECT id INTO DUMPFILE '/tmp/result' FROM items",
            "TABLE items INTO OUTFILE '/tmp/result'",
            "TABLE items INTO @picked",
            "WITH rows AS (SELECT 1) SELECT * INTO OUTFILE '/tmp/result' FROM rows",
        ] {
            let error = classify_mysql_multi_statement(sql, Some("app")).unwrap_err();
            assert!(error.contains("SELECT INTO clauses"), "{sql}: {error}");
        }
        assert!(
            classify_mysql_multi_statement(
                "WITH rows AS (SELECT 'INTO OUTFILE') SELECT * FROM rows",
                Some("app")
            )
            .is_ok()
        );
        assert!(
            classify_mysql_multi_statement(
                "SELECT id INTO @value FROM items; SELECT @value",
                Some("app")
            )
            .is_ok()
        );
    }

    #[test]
    fn allows_single_table_mutations_with_nested_table_references() {
        for sql in [
            "UPDATE items SET value = (SELECT MAX(value) FROM prices JOIN currencies ON prices.currency_id = currencies.id) WHERE id = 1",
            "DELETE FROM items WHERE id IN (SELECT item_id FROM prices JOIN currencies ON prices.currency_id = currencies.id)",
            "UPDATE items PARTITION (p0, p1) SET value = 1 WHERE id = 1",
            "UPDATE items USE INDEX FOR JOIN (idx_items) SET value = 1 WHERE id = 1",
            "DELETE FROM items PARTITION (p0, p1) WHERE id = 1",
        ] {
            assert!(classify_mysql_statement(sql).is_ok(), "{sql}");
        }
    }

    #[test]
    fn rejects_executable_inline_control_statement() {
        assert!(
            classify_mysql_statement("SELECT 1 /*!80000 SET sql_mode='ANSI_QUOTES' */").is_err()
        );
    }

    #[test]
    fn rejects_mysql_client_commands_at_line_start() {
        for sql in [
            "DELIMITER //\nSELECT 1//",
            "charset utf8mb4\nSELECT 1",
            "source ./script.sql",
            "system echo unsafe",
            "\\C /tmp/other.sock\nSELECT 1",
            "SELECT 1\nsource ./script.sql",
        ] {
            assert!(statement_contains_unsupported_mysql_control(sql), "{sql}");
        }
        assert!(!statement_contains_unsupported_mysql_control(
            "SELECT 'source ./script.sql\\n'"
        ));
    }

    #[test]
    fn rejects_mysql_client_commands_outside_literals_and_comments() {
        for sql in [
            r"SELECT 1 \G",
            r"SELECT 1\!",
            r"SELECT 1 \.",
            r"SELECT 1 \C utf8mb4",
        ] {
            assert!(statement_contains_unsupported_mysql_control(sql), "{sql}");
        }
        for sql in [
            r"SELECT '\G'",
            r"SELECT `\!`",
            r"SELECT 1 /* \.; \C */",
            "SELECT 1 -- \\!\n",
        ] {
            assert!(!statement_contains_unsupported_mysql_control(sql), "{sql}");
        }
    }

    #[test]
    fn keeps_index_confirmation_name_separate_from_ddl_database_target() {
        let statement = classify_mysql_statement("DROP INDEX ix ON app.items").unwrap();
        assert_eq!(statement.target, Some("ix".to_string()));
        assert_eq!(statement.target_database, Some("app".to_string()));
        let statement = classify_mysql_statement("DROP INDEX IF EXISTS ix ON app.items").unwrap();
        assert_eq!(statement.target, Some("ix".to_string()));
    }

    #[test]
    fn preserves_confirmation_target_case_for_unquoted_and_quoted_names() {
        let statement = classify_mysql_statement("DROP TABLE SalesOrder").unwrap();
        assert_eq!(statement.target, Some("SalesOrder".to_string()));

        let statement = classify_mysql_statement("DROP TABLE SalesDb.SalesOrder").unwrap();
        assert_eq!(statement.target, Some("SalesOrder".to_string()));
        assert_eq!(statement.target_database, Some("SalesDb".to_string()));

        let statement = classify_mysql_statement("DROP TABLE `SalesOrder`").unwrap();
        assert_eq!(statement.target, Some("SalesOrder".to_string()));
    }
}
