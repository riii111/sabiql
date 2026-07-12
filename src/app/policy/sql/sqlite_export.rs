use crate::domain::{DatabaseType, QuerySource};
use crate::policy::sql::sqlite_transaction::{
    SqliteStatementClassification, sqlite_statement_classification,
};
use crate::policy::sql::statement_classifier::first_keyword;
use crate::policy::write::sql_risk::{
    MultiStatementDecision, evaluate_multi_statement_for_database,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteExportPlan {
    RerunnableQuery { query: String },
    CachedResult { row_count: usize },
    NotExportable { reason: String },
}

pub fn sqlite_export_plan(
    source: QuerySource,
    query: &str,
    columns: &[String],
    row_count: usize,
) -> SqliteExportPlan {
    if source == QuerySource::Preview {
        return SqliteExportPlan::CachedResult { row_count };
    }
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
        MultiStatementDecision::Block { .. } => false,
        MultiStatementDecision::Allow { statements, .. } => {
            statements.len() == 1
                && statements
                    .iter()
                    .all(|statement| is_sqlite_rerunnable_export_statement(statement))
        }
    }
}

fn is_sqlite_rerunnable_export_statement(statement: &str) -> bool {
    if sqlite_statement_classification(statement) != SqliteStatementClassification::ReadOnly {
        return false;
    }
    matches!(
        first_keyword(statement).as_deref(),
        Some("SELECT" | "EXPLAIN" | "VALUES" | "WITH" | "PRAGMA")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod rerunnable {
        use super::*;

        #[test]
        fn plain_select_is_rerunnable() {
            assert!(is_sqlite_rerunnable_export_query("SELECT id FROM users"));
        }

        #[test]
        fn multi_select_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query("SELECT 1; SELECT 2"));
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

        #[test]
        fn persistent_pragma_write_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query(
                "PRAGMA user_version = 42"
            ));
        }

        #[test]
        fn maintenance_statement_is_not_rerunnable() {
            assert!(!is_sqlite_rerunnable_export_query("REINDEX users_name_idx"));
        }

        #[rstest]
        #[case::no_space_assignment("PRAGMA foreign_keys=OFF")]
        #[case::journal_mode("PRAGMA journal_mode=WAL")]
        #[case::parenthesized_checkpoint("PRAGMA wal_checkpoint(TRUNCATE)")]
        fn dangerous_pragma_variants_are_not_rerunnable(#[case] sql: &str) {
            assert!(!is_sqlite_rerunnable_export_query(sql));
        }
    }

    mod export_plan {
        use super::*;

        #[test]
        fn write_only_without_rows_is_not_exportable() {
            let plan = sqlite_export_plan(
                QuerySource::Adhoc,
                "INSERT INTO users(id) VALUES (1)",
                &[],
                0,
            );
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
                QuerySource::Adhoc,
                "INSERT INTO users(id) VALUES (1); SELECT id FROM users",
                &["id".to_string()],
                1,
            );
            assert_eq!(plan, SqliteExportPlan::CachedResult { row_count: 1 });
        }

        #[test]
        fn select_uses_rerunnable_query() {
            let plan = sqlite_export_plan(
                QuerySource::Adhoc,
                "SELECT id FROM users",
                &["id".to_string()],
                1,
            );
            assert!(matches!(plan, SqliteExportPlan::RerunnableQuery { .. }));
        }

        #[test]
        fn preview_uses_cached_result() {
            let plan = sqlite_export_plan(
                QuerySource::Preview,
                "SELECT __sabiql_rowid, CASE WHEN typeof(message) = 'text' THEN hex(message) END",
                &["message".to_string()],
                2,
            );

            assert_eq!(plan, SqliteExportPlan::CachedResult { row_count: 2 });
        }
    }
}
