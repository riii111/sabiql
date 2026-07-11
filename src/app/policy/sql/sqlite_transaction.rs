use crate::domain::DatabaseType;
use crate::policy::sql::statement_classifier::classify;
use crate::policy::write::sql_risk::{
    evaluate_sql_risk_for_database, split_statements_for_database,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteTransactionPolicy {
    AutoWrap,
    NotNeeded,
    UserManaged,
    IncompatibleStatement,
}

impl SqliteTransactionPolicy {
    pub fn requires_acknowledgement(self) -> bool {
        matches!(self, Self::IncompatibleStatement)
    }
}

pub fn sqlite_transaction_policy(sql: &str) -> SqliteTransactionPolicy {
    let statements = split_statements_for_database(DatabaseType::SQLite, sql);
    if statements.len() < 2
        || !statements.iter().any(|statement| {
            !evaluate_sql_risk_for_database(DatabaseType::SQLite, &classify(statement), statement)
                .read_only_allowed
        })
    {
        return SqliteTransactionPolicy::NotNeeded;
    }

    if statements
        .iter()
        .any(|statement| is_transaction_control(statement))
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

fn is_transaction_control(statement: &str) -> bool {
    matches!(
        first_keyword(statement).as_deref(),
        Some("BEGIN" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE")
    )
}

fn is_transaction_incompatible(statement: &str) -> bool {
    if first_keyword(statement).as_deref() == Some("VACUUM") {
        return true;
    }
    let Some((name, tail)) = pragma_name_and_tail(statement) else {
        return false;
    };
    matches!(name.as_str(), "journal_mode" | "foreign_keys")
        && (tail.contains('=') || tail.trim_start().starts_with('('))
}

fn first_keyword(statement: &str) -> Option<String> {
    statement
        .trim_start()
        .split(|ch: char| !ch.is_ascii_alphabetic())
        .next()
        .filter(|keyword| !keyword.is_empty())
        .map(str::to_ascii_uppercase)
}

fn pragma_name_and_tail(statement: &str) -> Option<(String, &str)> {
    let trimmed = statement.trim_start();
    if !trimmed.get(..6)?.eq_ignore_ascii_case("PRAGMA") {
        return None;
    }
    let tail = trimmed.get(6..)?.trim_start();
    let name_end = tail
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .unwrap_or(tail.len());
    let name = tail
        .get(..name_end)?
        .rsplit('.')
        .next()?
        .to_ascii_lowercase();
    Some((name, tail.get(name_end..)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incompatible_setters_require_an_acknowledgement() {
        assert_eq!(
            sqlite_transaction_policy("PRAGMA foreign_keys = OFF; CREATE TABLE users(id INTEGER)"),
            SqliteTransactionPolicy::IncompatibleStatement
        );
        assert_eq!(
            sqlite_transaction_policy("PRAGMA journal_mode(WAL); CREATE TABLE users(id INTEGER)"),
            SqliteTransactionPolicy::IncompatibleStatement
        );
    }
}
