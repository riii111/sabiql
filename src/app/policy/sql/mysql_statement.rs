use std::fmt;

mod classifier;
mod lexer;
mod side_effect;
mod target;
mod transaction;

use lexer::TokenKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MysqlStatementKind {
    Select,
    Table,
    Show,
    Describe,
    Insert,
    Replace,
    Update { has_where: bool },
    Delete { has_where: bool },
    CreateTable { temporary: bool },
    AlterTable,
    RenameTable,
    DropTable { temporary: bool },
    TruncateTable,
    CreateView,
    AlterView,
    DropView,
    CreateIndex,
    DropIndex,
    Begin,
    StartTransaction,
    Commit,
    Rollback,
    Savepoint,
    RollbackToSavepoint,
    ReleaseSavepoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MysqlStatement {
    pub sql: String,
    pub kind: MysqlStatementKind,
    pub target: Option<String>,
    pub target_database: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MysqlLexError(pub String);

impl fmt::Display for MysqlLexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn split_mysql_statements(sql: &str) -> Result<Vec<String>, MysqlLexError> {
    lexer::split_mysql_statements(sql)
}

pub fn classify_mysql_statement(sql: &str) -> Result<MysqlStatement, MysqlLexError> {
    let tokens = lexer::lex_mysql_statement(sql)?;
    if tokens.is_empty() {
        return Err(MysqlLexError("empty MySQL statement".to_string()));
    }
    if tokens.iter().any(|token| {
        matches!(
            &token.kind,
            TokenKind::Word(word) if word == "UNSUPPORTED_VERSION_COMMENT"
        )
    }) {
        return Err(MysqlLexError(
            "executable MySQL version comment contains another statement".to_string(),
        ));
    }
    let (kind, target, target_database) = classifier::kind_and_target(&tokens)?;
    Ok(MysqlStatement {
        sql: sql.to_string(),
        kind,
        target,
        target_database,
    })
}

pub fn has_top_level_into_clause(sql: &str) -> Result<bool, MysqlLexError> {
    side_effect::has_top_level_into_clause(sql)
}

pub fn has_mysql_read_only_side_effect(sql: &str) -> Result<bool, MysqlLexError> {
    side_effect::has_mysql_read_only_side_effect(sql)
}

pub fn has_mysql_version_comment(sql: &str) -> Result<bool, MysqlLexError> {
    side_effect::has_mysql_version_comment(sql)
}

pub fn target_is_selected_database(
    statement: &MysqlStatement,
    selected_database: Option<&str>,
) -> bool {
    target::target_is_selected_database(statement, selected_database)
}

pub fn statement_contains_unsupported_mysql_control(sql: &str) -> bool {
    side_effect::statement_contains_unsupported_mysql_control(sql)
}

pub fn mysql_explain_rejection_message(sql: &str) -> Option<&'static str> {
    let Ok(statements) = split_mysql_statements(sql) else {
        return Some("MySQL EXPLAIN requires one valid statement");
    };
    if statements.len() != 1 {
        return Some("MySQL EXPLAIN does not support multiple statements");
    }
    if statement_contains_unsupported_mysql_control(sql) {
        return Some("MySQL EXPLAIN does not support MySQL client commands");
    }

    let Ok(statement) = classify_mysql_statement(&statements[0]) else {
        return Some("MySQL EXPLAIN supports SELECT, TABLE, INSERT, UPDATE, or DELETE statements");
    };
    (!matches!(
        statement.kind,
        MysqlStatementKind::Select
            | MysqlStatementKind::Table
            | MysqlStatementKind::Insert
            | MysqlStatementKind::Update { .. }
            | MysqlStatementKind::Delete { .. }
    ))
    .then_some("MySQL EXPLAIN supports SELECT, TABLE, INSERT, UPDATE, or DELETE statements")
}

pub fn mysql_tree_explain_query_kind(sql: &str) -> Option<bool> {
    let trimmed = sql.trim();
    let (is_analyze, target) = strip_mysql_tree_explain_prefix(trimmed)?;
    let target = target.trim();

    let valid = if is_analyze {
        !statement_contains_unsupported_mysql_control(target)
            && split_mysql_statements(target).is_ok_and(|statements| {
                statements.len() == 1
                    && classify_mysql_statement(&statements[0]).is_ok_and(|statement| {
                        matches!(
                            statement.kind,
                            MysqlStatementKind::Select | MysqlStatementKind::Table
                        ) && !has_mysql_read_only_side_effect(target).unwrap_or(true)
                    })
            })
    } else {
        mysql_explain_rejection_message(target).is_none()
    };
    valid.then_some(is_analyze)
}

fn strip_mysql_tree_explain_prefix(text: &str) -> Option<(bool, &str)> {
    let mut index = 0;
    consume_mysql_keyword(text, &mut index, "EXPLAIN")?;
    consume_required_ascii_whitespace(text, &mut index)?;

    let is_analyze = if consume_mysql_keyword(text, &mut index, "ANALYZE").is_some() {
        consume_required_ascii_whitespace(text, &mut index)?;
        true
    } else {
        false
    };

    consume_mysql_keyword(text, &mut index, "FORMAT")?;
    skip_ascii_whitespace(text, &mut index);
    if text.as_bytes().get(index) != Some(&b'=') {
        return None;
    }
    index += 1;
    skip_ascii_whitespace(text, &mut index);
    consume_mysql_keyword(text, &mut index, "TREE")?;

    let target = text.get(index..)?;
    if target.is_empty() || !target.as_bytes()[0].is_ascii_whitespace() {
        return None;
    }
    Some((is_analyze, target))
}

fn consume_mysql_keyword(text: &str, index: &mut usize, keyword: &str) -> Option<()> {
    let candidate = text.get(*index..(*index).saturating_add(keyword.len()))?;
    if !candidate.eq_ignore_ascii_case(keyword) {
        return None;
    }
    if text
        .as_bytes()
        .get(index.saturating_add(keyword.len()))
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    {
        return None;
    }
    *index += keyword.len();
    Some(())
}

fn skip_ascii_whitespace(text: &str, index: &mut usize) {
    while text
        .as_bytes()
        .get(*index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        *index += 1;
    }
}

fn consume_required_ascii_whitespace(text: &str, index: &mut usize) -> Option<()> {
    let start = *index;
    skip_ascii_whitespace(text, index);
    (*index > start).then_some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_mysql_comments_quotes_and_backticks() {
        let sql = "SELECT 'a;\\'b'; # comment;\n SELECT `semi;colon` /* ; */";
        assert_eq!(split_mysql_statements(sql).unwrap().len(), 2);
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
        assert!(matches!(statement.kind, MysqlStatementKind::Update { .. }));
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
            Ok(MysqlStatement {
                kind: MysqlStatementKind::Select,
                ..
            })
        ));
    }

    #[test]
    fn classifies_documented_mysql_ddl_forms() {
        let cases = [
            (
                "RENAME TABLE app.items TO app.archived_items",
                MysqlStatementKind::RenameTable,
                "items",
                Some("app"),
            ),
            (
                "CREATE OR REPLACE VIEW app.item_view AS SELECT id FROM app.items",
                MysqlStatementKind::CreateView,
                "item_view",
                Some("app"),
            ),
            (
                "ALTER VIEW app.item_view AS SELECT id FROM app.items",
                MysqlStatementKind::AlterView,
                "item_view",
                Some("app"),
            ),
            (
                "CREATE FULLTEXT INDEX item_text ON app.items (body)",
                MysqlStatementKind::CreateIndex,
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
            MysqlStatementKind::CreateTable { temporary: false }
        );
        assert!(
            classify_mysql_statement("CREATE TABLE items (id INT) /* ordinary comment */").is_ok()
        );

        for sql in [
            "CREATE TABLE items (id INT) /*!40100 DEFAULT CHARSET=utf8mb4 */ SELECT 1",
            "CREATE TABLE items (id INT) /*!40100 SET sql_mode='ANSI_QUOTES' */",
            "CREATE TABLE items (id INT) /*!80000 DEFAULT CHARSET=utf8mb4 DROP TABLE other_items */",
            "DROP TABLE items /*!80000 , other_items */",
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
            "RENAME DATABASE app TO archive",
            "RENAME TABLE old_items TO archived_items, other_items TO other_archive",
            "RENAME TABLE app.items TO other.archived_items",
            "RENAME TABLE items TO app.archived_items",
            "CREATE OR REPLACE TABLE items (id INT)",
            "CREATE OR REPLACE INDEX item_index ON items (id)",
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
        ] {
            let error = classify_mysql_statement(sql).unwrap_err();
            assert!(error.0.contains("multiple-table"), "{sql}: {error}");
        }
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

    #[test]
    fn recognizes_generated_tree_explain_queries() {
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN FORMAT=TREE UPDATE items SET value = 1"),
            Some(false)
        );
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN ANALYZE FORMAT=TREE TABLE items"),
            Some(true)
        );
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN ANALYZE FORMAT=TREE DELETE FROM items"),
            None
        );
    }

    #[test]
    fn recognizes_tree_explain_queries_with_whitespace_around_equals() {
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN FORMAT = TREE UPDATE items SET value = 1"),
            Some(false)
        );
        assert_eq!(
            mysql_tree_explain_query_kind("EXPLAIN ANALYZE FORMAT = TREE TABLE items"),
            Some(true)
        );
    }

    #[test]
    fn tree_explain_prefix_keeps_keyword_boundaries_and_ignores_non_sql_text() {
        for sql in [
            "EXPLAIN FORMAT = JSON SELECT 1",
            "EXPLAIN FORMAT = TREEish SELECT 1",
            "EXPLAINFORMAT = TREE SELECT 1",
            "/* EXPLAIN FORMAT = TREE */ SELECT 1",
            "SELECT 'EXPLAIN FORMAT = TREE SELECT 1'",
        ] {
            assert_eq!(mysql_tree_explain_query_kind(sql), None, "{sql}");
        }
    }
}
