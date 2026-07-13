use crate::policy::sql::statement_classifier::{StatementKind, classify, first_keyword};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteStatementClassification {
    ReadOnly,
    TransactionalWrite,
    SessionSideEffect,
    TransactionIncompatible,
    TransactionControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlitePragma {
    pub name: String,
    pub has_value: bool,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteTransactionPolicy {
    AutoWrap,
    NotNeeded,
    UserManaged,
    IncompatibleStatement,
    ClassificationMismatch,
}

impl SqliteTransactionPolicy {
    pub fn requires_acknowledgement(self) -> bool {
        matches!(self, Self::IncompatibleStatement)
    }

    pub fn is_invalid(self) -> bool {
        matches!(self, Self::ClassificationMismatch)
    }
}

pub fn sqlite_transaction_policy(statements: &[String]) -> SqliteTransactionPolicy {
    let classifications: Vec<_> = statements
        .iter()
        .map(|statement| sqlite_statement_classification(statement))
        .collect();
    sqlite_transaction_policy_for_classifications(statements.len(), &classifications)
}

pub fn sqlite_transaction_policy_for_classifications(
    statement_count: usize,
    classifications: &[SqliteStatementClassification],
) -> SqliteTransactionPolicy {
    if statement_count != classifications.len() {
        return SqliteTransactionPolicy::ClassificationMismatch;
    }
    if statement_count < 2 {
        return SqliteTransactionPolicy::NotNeeded;
    }
    if classifications.iter().any(|classification| {
        matches!(
            classification,
            SqliteStatementClassification::TransactionControl
        )
    }) {
        return SqliteTransactionPolicy::UserManaged;
    }
    if classifications.iter().any(|classification| {
        matches!(
            classification,
            SqliteStatementClassification::SessionSideEffect
                | SqliteStatementClassification::TransactionIncompatible
        )
    }) {
        return SqliteTransactionPolicy::IncompatibleStatement;
    }
    if classifications.iter().any(|classification| {
        matches!(
            classification,
            SqliteStatementClassification::TransactionalWrite
        )
    }) {
        SqliteTransactionPolicy::AutoWrap
    } else {
        SqliteTransactionPolicy::NotNeeded
    }
}

pub fn sqlite_statement_classification(statement: &str) -> SqliteStatementClassification {
    if matches!(classify(statement), StatementKind::Transaction) {
        return SqliteStatementClassification::TransactionControl;
    }
    if is_transaction_incompatible(statement) {
        return SqliteStatementClassification::TransactionIncompatible;
    }
    if is_transactional_pragma_write(statement) {
        return SqliteStatementClassification::TransactionalWrite;
    }
    if is_session_pragma_side_effect(statement) {
        return SqliteStatementClassification::SessionSideEffect;
    }
    if matches!(
        first_keyword(statement).as_deref(),
        Some("ATTACH" | "DETACH")
    ) {
        return SqliteStatementClassification::SessionSideEffect;
    }
    if matches!(
        first_keyword(statement).as_deref(),
        Some("ANALYZE" | "REINDEX" | "REPLACE")
    ) {
        return SqliteStatementClassification::TransactionalWrite;
    }
    if matches!(
        classify(statement),
        StatementKind::Insert
            | StatementKind::Update { .. }
            | StatementKind::Delete { .. }
            | StatementKind::Create
            | StatementKind::Alter
            | StatementKind::Drop
            | StatementKind::Truncate
    ) {
        SqliteStatementClassification::TransactionalWrite
    } else {
        SqliteStatementClassification::ReadOnly
    }
}

pub fn is_transaction_incompatible(statement: &str) -> bool {
    if first_keyword(statement).as_deref() == Some("VACUUM") {
        return true;
    }
    let Some(pragma) = parse_sqlite_pragma(statement) else {
        return false;
    };
    matches!(
        pragma.name.as_str(),
        "journal_mode" | "foreign_keys" | "synchronous"
    ) && pragma.has_value
}

fn is_transactional_pragma_write(statement: &str) -> bool {
    let Some(pragma) = parse_sqlite_pragma(statement) else {
        return false;
    };
    matches!(pragma.name.as_str(), "application_id" | "user_version") && pragma.has_value
}

fn is_session_pragma_side_effect(statement: &str) -> bool {
    let Some(pragma) = parse_sqlite_pragma(statement) else {
        return false;
    };
    (pragma.has_value && !is_read_only_parameterized_pragma(&pragma.name))
        || matches!(
            pragma.name.as_str(),
            "optimize" | "incremental_vacuum" | "wal_checkpoint"
        )
}

fn is_read_only_parameterized_pragma(name: &str) -> bool {
    matches!(
        name,
        "table_info"
            | "table_xinfo"
            | "index_info"
            | "index_xinfo"
            | "index_list"
            | "foreign_key_list"
            | "database_list"
            | "table_list"
            | "pragma_list"
            | "function_list"
            | "module_list"
            | "collation_list"
            | "integrity_check"
            | "quick_check"
            | "column_info"
    )
}

fn trim_sql_prefix(mut sql: &str) -> &str {
    loop {
        let trimmed = sql.trim_start();
        if let Some(comment) = trimmed.strip_prefix("--") {
            sql = comment.find('\n').map_or("", |index| &comment[index + 1..]);
            continue;
        }
        if let Some(comment) = trimmed.strip_prefix("/*") {
            sql = comment.find("*/").map_or("", |index| &comment[index + 2..]);
            continue;
        }
        return trimmed;
    }
}

pub fn parse_sqlite_pragma(statement: &str) -> Option<SqlitePragma> {
    let trimmed = trim_sql_prefix(statement);
    if !trimmed.get(..6)?.eq_ignore_ascii_case("PRAGMA") {
        return None;
    }
    let tail = trim_sql_prefix(trimmed.get(6..)?);
    let (first_name, rest) = pragma_identifier_and_tail(tail)?;
    let rest = trim_sql_prefix(rest);
    let (name, rest) = if let Some(rest) = rest.strip_prefix('.') {
        let (name, rest) = pragma_identifier_and_tail(trim_sql_prefix(rest))?;
        (name, rest)
    } else {
        (first_name, rest)
    };
    let rest = trim_sql_prefix(rest);
    let has_value = rest.starts_with('=') || rest.starts_with('(');
    let value = pragma_value(rest);
    Some(SqlitePragma {
        name: name.to_ascii_lowercase(),
        has_value,
        value,
    })
}

fn pragma_value(tail: &str) -> Option<String> {
    let value = if let Some(value) = tail.strip_prefix('=') {
        value
    } else {
        let value = tail.strip_prefix('(')?;
        &value[..value.find(')')?]
    };
    value
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .find(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
}

fn pragma_identifier_and_tail(sql: &str) -> Option<(&str, &str)> {
    let (name, rest) = match sql.as_bytes().first()? {
        b'"' | b'\'' | b'`' => {
            let quote = sql.as_bytes()[0] as char;
            let end = sql[1..].find(quote)? + 1;
            (sql.get(1..end)?, sql.get(end + 1..)?)
        }
        b'[' => {
            let end = sql.find(']')?;
            (sql.get(1..end)?, sql.get(end + 1..)?)
        }
        _ => {
            let end = sql
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .unwrap_or(sql.len());
            (sql.get(..end)?, sql.get(end..)?)
        }
    };
    (!name.is_empty()).then_some((name, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DatabaseType;
    use crate::policy::write::sql_risk::split_statements_for_database;

    fn policy_for(sql: &str) -> SqliteTransactionPolicy {
        let statements = split_statements_for_database(DatabaseType::SQLite, sql);
        sqlite_transaction_policy(&statements)
    }

    #[test]
    fn incompatible_setters_require_an_acknowledgement() {
        assert_eq!(
            policy_for("PRAGMA foreign_keys = OFF; CREATE TABLE users(id INTEGER)"),
            SqliteTransactionPolicy::IncompatibleStatement
        );
        assert_eq!(
            policy_for("PRAGMA journal_mode(WAL); CREATE TABLE users(id INTEGER)"),
            SqliteTransactionPolicy::IncompatibleStatement
        );
    }

    #[test]
    fn comments_and_quoted_pragma_names_are_classified() {
        for sql in [
            "-- setup\nPRAGMA foreign_keys=OFF; CREATE TABLE users(id INTEGER)",
            "PRAGMA \"foreign_keys\"=OFF; CREATE TABLE users(id INTEGER)",
            "PRAGMA [foreign_keys](OFF); CREATE TABLE users(id INTEGER)",
        ] {
            assert_eq!(
                policy_for(sql),
                SqliteTransactionPolicy::IncompatibleStatement,
                "{sql}"
            );
        }
    }

    #[test]
    fn vacuum_is_transaction_incompatible() {
        assert!(is_transaction_incompatible("VACUUM"));
        assert!(is_transaction_incompatible("  VACUUM"));
        assert!(is_transaction_incompatible("VACUUM INTO 'backup.db'"));
    }

    #[test]
    fn query_pragma_is_not_transaction_incompatible() {
        assert!(!is_transaction_incompatible("PRAGMA foreign_keys"));
        assert!(!is_transaction_incompatible("PRAGMA journal_mode"));
    }

    #[test]
    fn classification_mismatch_is_not_treated_as_not_needed() {
        assert_eq!(
            sqlite_transaction_policy_for_classifications(1, &[]),
            SqliteTransactionPolicy::ClassificationMismatch
        );
    }

    #[test]
    fn persistent_pragma_writes_are_transactional() {
        for sql in [
            "PRAGMA user_version = 42",
            "PRAGMA application_id(7)",
            "PRAGMA \"main\".\"user_version\" = 42",
            "PRAGMA [main].[application_id](7)",
        ] {
            assert_eq!(
                sqlite_statement_classification(sql),
                SqliteStatementClassification::TransactionalWrite,
                "{sql}"
            );
        }
    }

    #[test]
    fn persistent_pragma_write_enables_auto_wrap_for_multi_statement_sql() {
        let statements = vec![
            "PRAGMA user_version = 42".to_string(),
            "SELECT * FROM missing_table".to_string(),
        ];

        assert_eq!(
            sqlite_transaction_policy(&statements),
            SqliteTransactionPolicy::AutoWrap
        );
    }

    #[test]
    fn side_effect_pragma_requires_acknowledgement_without_a_transactional_write() {
        for sql in [
            "PRAGMA synchronous = NORMAL; SELECT 1",
            "PRAGMA cache_size = 2000; SELECT 1",
        ] {
            assert_eq!(
                policy_for(sql),
                SqliteTransactionPolicy::IncompatibleStatement,
                "{sql}"
            );
        }
    }

    #[test]
    fn session_pragma_changes_are_not_implicitly_atomic() {
        for sql in [
            "PRAGMA cache_size = 2000",
            "PRAGMA locking_mode = EXCLUSIVE",
        ] {
            assert_eq!(
                sqlite_statement_classification(sql),
                SqliteStatementClassification::SessionSideEffect,
                "{sql}"
            );
        }
    }

    #[test]
    fn synchronous_change_is_transaction_incompatible() {
        assert_eq!(
            sqlite_statement_classification("PRAGMA synchronous = NORMAL"),
            SqliteStatementClassification::TransactionIncompatible
        );
    }

    #[test]
    fn quoted_schema_pragma_is_normalized_with_its_value() {
        for (sql, expected_name, expected_value) in [
            (
                "PRAGMA \"main\".\"foreign_keys\" = OFF",
                "foreign_keys",
                Some("off"),
            ),
            (
                "PRAGMA [main].[journal_mode](WAL)",
                "journal_mode",
                Some("wal"),
            ),
        ] {
            let pragma = parse_sqlite_pragma(sql).unwrap();

            assert_eq!(pragma.name, expected_name, "{sql}");
            assert_eq!(pragma.value.as_deref(), expected_value, "{sql}");
        }
    }
}
