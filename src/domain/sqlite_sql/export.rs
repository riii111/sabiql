use super::splitter::first_sqlite_keyword;
use super::transaction::{SqliteStatementClassification, sqlite_statement_classification};

pub fn is_sqlite_rerunnable_export_statement(statement: &str) -> bool {
    if sqlite_statement_classification(statement) != SqliteStatementClassification::ReadOnly {
        return false;
    }
    matches!(
        first_sqlite_keyword(statement).as_deref(),
        Some("SELECT" | "EXPLAIN" | "VALUES" | "WITH" | "PRAGMA")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_read_only_statements() {
        for sql in ["SELECT id FROM users", "PRAGMA table_info(users)"] {
            assert!(is_sqlite_rerunnable_export_statement(sql), "{sql}");
        }
    }

    #[test]
    fn rejects_writes() {
        for sql in [
            "INSERT INTO users(id) VALUES (1)",
            "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload",
            "WITH payload(id) AS (VALUES (1)) REPLACE INTO users(id) SELECT id FROM payload",
            "PRAGMA foreign_keys = OFF",
            "PRAGMA journal_mode = WAL",
        ] {
            assert!(!is_sqlite_rerunnable_export_statement(sql), "{sql}");
        }
    }
}
