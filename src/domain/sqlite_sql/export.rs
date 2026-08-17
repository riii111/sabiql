use super::splitter::{first_sqlite_keyword, split_sqlite_statements};
use super::transaction::{SqliteStatementClassification, sqlite_statement_classification};

pub fn is_sqlite_rerunnable_export_query(query: &str) -> bool {
    let split = split_sqlite_statements(query);
    let statements = split
        .statements()
        .iter()
        .copied()
        .filter(|statement| !is_comment_only(statement))
        .collect::<Vec<_>>();
    statements.len() == 1
        && statements
            .iter()
            .all(|statement| is_sqlite_rerunnable_export_statement(statement))
}

pub fn is_sqlite_rerunnable_export_statement(statement: &str) -> bool {
    if sqlite_statement_classification(statement) != SqliteStatementClassification::ReadOnly {
        return false;
    }
    matches!(
        first_sqlite_keyword(statement).as_deref(),
        Some("SELECT" | "EXPLAIN" | "VALUES" | "WITH" | "PRAGMA")
    )
}

fn is_comment_only(statement: &str) -> bool {
    let mut rest = statement.trim_start();
    loop {
        if let Some(comment) = rest.strip_prefix("--") {
            rest = comment.find('\n').map_or("", |index| &comment[index + 1..]);
            continue;
        }
        if let Some(comment) = rest.strip_prefix("/*") {
            rest = comment.find("*/").map_or("", |index| &comment[index + 2..]);
            continue;
        }
        return rest.trim().is_empty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_single_read_only_query() {
        assert!(is_sqlite_rerunnable_export_query("SELECT id FROM users"));
        assert!(is_sqlite_rerunnable_export_query(
            "PRAGMA table_info(users)"
        ));
    }

    #[test]
    fn rejects_writes_and_multiple_statements() {
        for sql in [
            "INSERT INTO users(id) VALUES (1)",
            "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload",
            "SELECT 1; SELECT 2",
            "PRAGMA foreign_keys = OFF",
            "PRAGMA journal_mode = WAL",
        ] {
            assert!(!is_sqlite_rerunnable_export_query(sql), "{sql}");
        }
    }

    #[test]
    fn ignores_comment_only_statement_fragments() {
        assert!(is_sqlite_rerunnable_export_query(
            "SELECT 1; -- trailing comment"
        ));
    }
}
