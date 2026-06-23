use crate::domain::DatabaseType;
use crate::policy::sql::statement_classifier::{classify, first_keyword};
use crate::policy::write::sql_risk::{
    evaluate_multi_statement_for_database, evaluate_sql_risk_for_database,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteExportPlan {
    RerunnableQuery { query: String },
    CachedResult { row_count: usize },
    NotExportable { reason: String },
}

pub fn sqlite_export_plan(query: &str, columns: &[String], row_count: usize) -> SqliteExportPlan {
    if is_sqlite_rerunnable_export_query(query) {
        return SqliteExportPlan::RerunnableQuery {
            query: query.to_string(),
        };
    }
    if columns.is_empty() {
        return SqliteExportPlan::NotExportable {
            reason: "Cannot export: query contains write or DDL statements and produced no tabular result".to_string(),
        };
    }
    SqliteExportPlan::CachedResult { row_count }
}

pub fn is_sqlite_rerunnable_export_query(query: &str) -> bool {
    match evaluate_multi_statement_for_database(DatabaseType::SQLite, query) {
        crate::policy::write::sql_risk::MultiStatementDecision::Block { .. } => false,
        crate::policy::write::sql_risk::MultiStatementDecision::Allow { statements, .. } => {
            statements
                .iter()
                .all(|statement| is_sqlite_rerunnable_export_statement(statement))
        }
    }
}

fn is_sqlite_rerunnable_export_statement(statement: &str) -> bool {
    if is_write_statement(statement)
        || is_dml_statement(statement)
        || is_transaction_control(statement)
    {
        return false;
    }
    let kind = classify(statement);
    evaluate_sql_risk_for_database(DatabaseType::SQLite, &kind, statement).read_only_allowed
}

fn is_write_statement(statement: &str) -> bool {
    matches!(
        first_keyword(statement).as_deref(),
        Some("INSERT" | "REPLACE" | "UPDATE" | "DELETE" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE")
    )
}

fn is_transaction_control(statement: &str) -> bool {
    matches!(
        first_keyword(statement).as_deref(),
        Some("BEGIN" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE")
    )
}

fn is_dml_statement(statement: &str) -> bool {
    dml_keyword(statement).is_some()
}

fn dml_keyword(statement: &str) -> Option<&'static str> {
    let keyword = first_keyword(statement)?;
    if keyword.eq_ignore_ascii_case("INSERT") {
        return Some("INSERT");
    }
    if keyword.eq_ignore_ascii_case("REPLACE") {
        return Some("INSERT");
    }
    if keyword.eq_ignore_ascii_case("UPDATE") {
        return Some("UPDATE");
    }
    if keyword.eq_ignore_ascii_case("DELETE") {
        return Some("DELETE");
    }
    if !keyword.eq_ignore_ascii_case("WITH") {
        return None;
    }

    let lower = statement.to_lowercase();
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    let mut offset = 0usize;
    while let Some((next_keyword, end)) = next_keyword_from(&lower, &chars, offset) {
        if next_keyword.eq_ignore_ascii_case("INSERT") {
            return Some("INSERT");
        }
        if next_keyword.eq_ignore_ascii_case("REPLACE") {
            return Some("INSERT");
        }
        if next_keyword.eq_ignore_ascii_case("UPDATE") {
            return Some("UPDATE");
        }
        if next_keyword.eq_ignore_ascii_case("DELETE") {
            return Some("DELETE");
        }
        offset = end;
    }
    None
}

fn next_keyword_from(
    lower: &str,
    chars: &[(usize, char)],
    start: usize,
) -> Option<(String, usize)> {
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].0 < start {
            i += 1;
            continue;
        }
        if chars[i].1.is_ascii_alphabetic() {
            let keyword_start = chars[i].0;
            let mut j = i + 1;
            while j < chars.len() && (chars[j].1.is_ascii_alphanumeric() || chars[j].1 == '_') {
                j += 1;
            }
            let keyword = lower[keyword_start..chars[j - 1].0 + chars[j - 1].1.len_utf8()]
                .to_ascii_uppercase();
            return Some((keyword, chars[j - 1].0 + chars[j - 1].1.len_utf8()));
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    mod rerunnable {
        use super::*;

        #[test]
        fn plain_select_is_rerunnable() {
            assert!(is_sqlite_rerunnable_export_query("SELECT id FROM users"));
        }

        #[test]
        fn multi_select_is_rerunnable() {
            assert!(is_sqlite_rerunnable_export_query("SELECT 1; SELECT 2"));
        }

        #[test]
        fn read_only_pragma_is_rerunnable() {
            assert!(is_sqlite_rerunnable_export_query(
                "PRAGMA table_info(users)"
            ));
        }
    }

    mod not_rerunnable {
        use super::*;

        #[test]
        fn insert_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query(
                "INSERT INTO users(id) VALUES (1)"
            ));
        }

        #[test]
        fn mixed_write_and_select_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query(
                "INSERT INTO users(id) VALUES (1); SELECT * FROM users"
            ));
        }

        #[test]
        fn with_dml_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query(
                "WITH payload(id) AS (VALUES (1)) INSERT INTO users(id) SELECT id FROM payload"
            ));
        }

        #[test]
        fn ddl_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query(
                "CREATE TABLE backup AS SELECT * FROM users"
            ));
        }

        #[test]
        fn write_pragma_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query(
                "PRAGMA foreign_keys = OFF"
            ));
        }
    }

    mod export_plan {
        use super::*;

        #[test]
        fn write_only_without_rows_is_not_exportable() {
            let plan = sqlite_export_plan("INSERT INTO users(id) VALUES (1)", &[], 0);
            assert_eq!(
                plan,
                SqliteExportPlan::NotExportable {
                    reason: "Cannot export: query contains write or DDL statements and produced no tabular result".to_string(),
                }
            );
        }

        #[test]
        fn mixed_query_with_rows_uses_cached_result() {
            let plan = sqlite_export_plan(
                "INSERT INTO users(id) VALUES (1); SELECT id FROM users",
                &["id".to_string()],
                1,
            );
            assert_eq!(plan, SqliteExportPlan::CachedResult { row_count: 1 });
        }

        #[test]
        fn select_uses_rerunnable_query() {
            let plan = sqlite_export_plan("SELECT id FROM users", &["id".to_string()], 1);
            assert!(matches!(plan, SqliteExportPlan::RerunnableQuery { .. }));
        }
    }
}
