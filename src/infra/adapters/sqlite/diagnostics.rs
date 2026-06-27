use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, QueryExecutor, SqliteDiagnosticsProvider};
use crate::domain::{DiagnosticField, QueryResult, SqliteDiagnosticsSnapshot};

use super::SqliteAdapter;

#[async_trait]
impl SqliteDiagnosticsProvider for SqliteAdapter {
    async fn fetch_diagnostics_core(
        &self,
        dsn: &str,
        read_only: bool,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
        let db_file = DiagnosticField::ok(Self::path_from_dsn(dsn)?);

        let sqlite_version = fetch_field(
            self,
            dsn,
            read_only,
            "SELECT sqlite_version();",
            scalar_field,
        )
        .await;
        let foreign_keys =
            fetch_field(self, dsn, read_only, "PRAGMA foreign_keys;", on_off_field).await;
        let journal_mode =
            fetch_field(self, dsn, read_only, "PRAGMA journal_mode;", scalar_field).await;
        let query_only =
            fetch_field(self, dsn, read_only, "PRAGMA query_only;", on_off_field).await;
        let busy_timeout =
            fetch_field(self, dsn, read_only, "PRAGMA busy_timeout;", scalar_field).await;
        let database_list = fetch_field(
            self,
            dsn,
            read_only,
            "PRAGMA database_list;",
            database_list_field,
        )
        .await;

        Ok(SqliteDiagnosticsSnapshot {
            db_file,
            sqlite_version,
            foreign_keys,
            journal_mode,
            query_only,
            busy_timeout,
            database_list,
            quick_check: DiagnosticField::default(),
        })
    }

    async fn fetch_quick_check(
        &self,
        dsn: &str,
        read_only: bool,
    ) -> Result<DiagnosticField, DbOperationError> {
        Ok(fetch_field(
            self,
            dsn,
            read_only,
            "PRAGMA quick_check;",
            quick_check_field,
        )
        .await)
    }
}

async fn fetch_field(
    adapter: &SqliteAdapter,
    dsn: &str,
    read_only: bool,
    query: &str,
    map: fn(Result<QueryResult, DbOperationError>) -> DiagnosticField,
) -> DiagnosticField {
    map(adapter.execute_adhoc(dsn, query, read_only).await)
}

fn scalar_field(result: Result<QueryResult, DbOperationError>) -> DiagnosticField {
    match result {
        Ok(query_result) => match first_cell(&query_result) {
            Ok(value) => DiagnosticField::ok(value),
            Err(error) => DiagnosticField::err(error),
        },
        Err(error) => DiagnosticField::err(error.masked_details()),
    }
}

fn on_off_field(result: Result<QueryResult, DbOperationError>) -> DiagnosticField {
    match result {
        Ok(query_result) => match first_cell(&query_result) {
            Ok(value) => DiagnosticField::ok(format_on_off(&value)),
            Err(error) => DiagnosticField::err(error),
        },
        Err(error) => DiagnosticField::err(error.masked_details()),
    }
}

fn database_list_field(result: Result<QueryResult, DbOperationError>) -> DiagnosticField {
    match result {
        Ok(query_result) => {
            if query_result.rows().is_empty() {
                return DiagnosticField::err("database_list: empty result");
            }
            let lines = query_result
                .rows()
                .iter()
                .map(|row| format_database_list_row(row.as_slice()))
                .collect::<Vec<_>>()
                .join("\n");
            DiagnosticField::ok(lines)
        }
        Err(error) => DiagnosticField::err(error.masked_details()),
    }
}

fn quick_check_field(result: Result<QueryResult, DbOperationError>) -> DiagnosticField {
    match result {
        Ok(query_result) => {
            if query_result.rows().is_empty() {
                return DiagnosticField::err("quick_check: empty result");
            }
            let summary = query_result
                .rows()
                .iter()
                .filter_map(|row| row.first())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            DiagnosticField::ok(summary)
        }
        Err(error) => DiagnosticField::err(error.masked_details()),
    }
}

fn first_cell(result: &QueryResult) -> Result<String, String> {
    result
        .rows()
        .first()
        .and_then(|row| row.first())
        .cloned()
        .ok_or_else(|| "empty result".to_string())
}

fn format_on_off(value: &str) -> String {
    match value.trim() {
        "0" => "off".to_string(),
        "1" => "on".to_string(),
        other => other.to_string(),
    }
}

fn format_database_list_row(row: &[String]) -> String {
    match row {
        [seq, name, file] => format!("{seq}: {name} @ {file}"),
        _ => row.join(" | "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::test_support::make_sqlite_db;
    use crate::domain::{QueryResult, QuerySource};

    fn empty_query_result() -> QueryResult {
        QueryResult::success(String::new(), vec![], vec![], 0, QuerySource::Adhoc)
    }

    #[tokio::test]
    async fn fetch_diagnostics_core_reports_pragmas_without_quick_check() {
        let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let snapshot = adapter.fetch_diagnostics_core(&dsn, true).await.unwrap();

        assert!(snapshot.db_file.is_ok());
        assert!(snapshot.sqlite_version.is_ok());
        assert!(snapshot.foreign_keys.is_ok());
        assert!(snapshot.journal_mode.is_ok());
        assert!(snapshot.query_only.is_ok());
        assert!(snapshot.busy_timeout.is_ok());
        assert!(snapshot.database_list.is_ok());
        assert_eq!(snapshot.quick_check.value, None);
    }

    #[tokio::test]
    async fn fetch_quick_check_reports_integrity_summary() {
        let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let quick_check = adapter.fetch_quick_check(&dsn, true).await.unwrap();

        assert!(quick_check.is_ok());
        assert!(
            quick_check
                .value
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("ok"))
        );
    }

    #[test]
    fn scalar_field_maps_query_failure_to_error() {
        let field = scalar_field(Err(DbOperationError::QueryFailed("boom".to_string())));

        assert!(field.error.is_some());
    }

    #[test]
    fn scalar_field_maps_empty_rows_to_error() {
        let field = scalar_field(Ok(empty_query_result()));

        assert_eq!(field.error.as_deref(), Some("empty result"));
    }

    #[test]
    fn quick_check_field_maps_empty_rows_to_error() {
        let field = quick_check_field(Ok(empty_query_result()));

        assert_eq!(field.error.as_deref(), Some("quick_check: empty result"));
    }

    #[test]
    fn database_list_field_maps_empty_rows_to_error() {
        let field = database_list_field(Ok(empty_query_result()));

        assert_eq!(field.error.as_deref(), Some("database_list: empty result"));
    }

    #[test]
    fn format_on_off_maps_sqlite_integers() {
        assert_eq!(format_on_off("1"), "on");
        assert_eq!(format_on_off("0"), "off");
    }
}
