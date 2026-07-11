use crate::policy::sql::statement_classifier::{StatementKind, first_keyword};

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

pub fn sqlite_transaction_policy(
    statements: &[String],
    statement_kinds: &[StatementKind],
    has_write: bool,
) -> SqliteTransactionPolicy {
    if statements.len() != statement_kinds.len() {
        return SqliteTransactionPolicy::ClassificationMismatch;
    }
    if statements.len() < 2 || !has_write {
        return SqliteTransactionPolicy::NotNeeded;
    }

    if statement_kinds
        .iter()
        .any(|kind| matches!(kind, StatementKind::Transaction))
    {
        return SqliteTransactionPolicy::UserManaged;
    }
    if statements
        .iter()
        .any(|statement| is_transaction_incompatible(statement))
    {
        return SqliteTransactionPolicy::IncompatibleStatement;
    }
    SqliteTransactionPolicy::AutoWrap
}

pub fn is_transaction_incompatible(statement: &str) -> bool {
    if first_keyword(statement).as_deref() == Some("VACUUM") {
        return true;
    }
    let Some((name, tail)) = pragma_name_and_tail(statement) else {
        return false;
    };
    matches!(name.as_str(), "journal_mode" | "foreign_keys") && pragma_has_value(tail)
}

fn pragma_has_value(tail: &str) -> bool {
    let tail = trim_sql_prefix(tail);
    tail.starts_with('=') || tail.starts_with('(')
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

fn pragma_name_and_tail(statement: &str) -> Option<(String, &str)> {
    let trimmed = trim_sql_prefix(statement);
    if !trimmed.get(..6)?.eq_ignore_ascii_case("PRAGMA") {
        return None;
    }
    let tail = trim_sql_prefix(trimmed.get(6..)?);
    let (name, rest) = match tail.as_bytes().first()? {
        b'"' | b'\'' | b'`' => {
            let quote = tail.as_bytes()[0] as char;
            let end = tail[1..].find(quote)? + 1;
            (tail.get(1..end)?, tail.get(end + 1..)?)
        }
        b'[' => {
            let end = tail.find(']')?;
            (tail.get(1..end)?, tail.get(end + 1..)?)
        }
        _ => {
            let end = tail
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                .unwrap_or(tail.len());
            (tail.get(..end)?, tail.get(end..)?)
        }
    };
    Some((name.rsplit('.').next()?.to_ascii_lowercase(), rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DatabaseType;
    use crate::policy::sql::statement_classifier::classify;
    use crate::policy::write::sql_risk::split_statements_for_database;

    fn policy_for(sql: &str) -> SqliteTransactionPolicy {
        let statements = split_statements_for_database(DatabaseType::SQLite, sql);
        let kinds: Vec<_> = statements
            .iter()
            .map(|statement| classify(statement))
            .collect();
        sqlite_transaction_policy(&statements, &kinds, true)
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
    fn query_pragma_is_not_transaction_incompatible() {
        assert!(!is_transaction_incompatible("PRAGMA foreign_keys"));
        assert!(!is_transaction_incompatible("PRAGMA journal_mode"));
    }

    #[test]
    fn classification_mismatch_is_not_treated_as_not_needed() {
        let statements = vec!["INSERT INTO users(id) VALUES (1)".to_string()];
        assert_eq!(
            sqlite_transaction_policy(&statements, &[], true),
            SqliteTransactionPolicy::ClassificationMismatch
        );
    }
}
