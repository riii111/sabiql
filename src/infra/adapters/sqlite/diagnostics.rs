#[cfg(test)]
use crate::adapters::test_support;

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
        let feature_summary = fetch_feature_summary(self, dsn, read_only).await;
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
            feature_summary,
            foreign_keys,
            journal_mode,
            query_only,
            busy_timeout,
            database_list,
            quick_check: DiagnosticField::Pending,
        })
    }

    async fn fetch_quick_check(&self, dsn: &str, read_only: bool) -> DiagnosticField {
        fetch_field(
            self,
            dsn,
            read_only,
            "PRAGMA quick_check;",
            quick_check_field,
        )
        .await
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

async fn fetch_feature_summary(
    adapter: &SqliteAdapter,
    dsn: &str,
    read_only: bool,
) -> DiagnosticField {
    let compile_options = feature_probe(
        adapter
            .execute_adhoc(dsn, "PRAGMA compile_options;", read_only)
            .await,
    );
    let module_list = feature_probe(
        adapter
            .execute_adhoc(dsn, "PRAGMA module_list;", read_only)
            .await,
    );
    let function_list = feature_probe(
        adapter
            .execute_adhoc(dsn, "PRAGMA function_list;", read_only)
            .await,
    );

    feature_summary_field(&compile_options, &module_list, &function_list)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FeatureProbe {
    Available(Vec<String>),
    Unavailable,
}

impl FeatureProbe {
    fn values(&self) -> Option<&[String]> {
        match self {
            Self::Available(values) => Some(values.as_slice()),
            Self::Unavailable => None,
        }
    }
}

fn feature_probe(result: Result<QueryResult, DbOperationError>) -> FeatureProbe {
    match result {
        Ok(query_result) if query_result.columns.is_empty() => FeatureProbe::Unavailable,
        Ok(query_result) => FeatureProbe::Available(
            query_result
                .rows()
                .iter()
                .filter_map(|row| row.first())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
        ),
        Err(_) => FeatureProbe::Unavailable,
    }
}

fn feature_summary_field(
    compile_options: &FeatureProbe,
    module_list: &FeatureProbe,
    function_list: &FeatureProbe,
) -> DiagnosticField {
    let features = [
        (
            "FTS5",
            feature_from_compile_or_module(compile_options, module_list, "ENABLE_FTS5", "fts5"),
        ),
        (
            "FTS4",
            feature_from_compile_or_module(compile_options, module_list, "ENABLE_FTS4", "fts4"),
        ),
        (
            "RTree",
            feature_from_compile_or_module(compile_options, module_list, "ENABLE_RTREE", "rtree"),
        ),
        ("JSON", json_feature(compile_options, function_list)),
    ];

    if features
        .iter()
        .all(|(_, availability)| availability.is_none())
    {
        return DiagnosticField::Unavailable;
    }

    DiagnosticField::ok(
        features
            .into_iter()
            .map(|(name, availability)| format!("{name}: {}", format_feature(availability)))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn feature_from_compile_or_module(
    compile_options: &FeatureProbe,
    module_list: &FeatureProbe,
    compile_option: &str,
    module_name: &str,
) -> Option<bool> {
    if contains_ignore_ascii_case(compile_options.values(), compile_option)
        || contains_ignore_ascii_case(module_list.values(), module_name)
    {
        return Some(true);
    }

    compile_options.values().map(|_| false)
}

fn json_feature(compile_options: &FeatureProbe, function_list: &FeatureProbe) -> Option<bool> {
    if contains_ignore_ascii_case(compile_options.values(), "ENABLE_JSON1")
        || function_list.values().is_some_and(|functions| {
            functions.iter().any(|function| {
                let function = function.to_ascii_lowercase();
                function == "json" || function.starts_with("json_")
            })
        })
    {
        return Some(true);
    }
    if contains_ignore_ascii_case(compile_options.values(), "OMIT_JSON") {
        return Some(false);
    }

    function_list.values().map(|_| false)
}

fn contains_ignore_ascii_case(values: Option<&[String]>, needle: &str) -> bool {
    values.is_some_and(|values| {
        values
            .iter()
            .any(|value| value.eq_ignore_ascii_case(needle))
    })
}

fn format_feature(availability: Option<bool>) -> &'static str {
    match availability {
        Some(true) => "available",
        Some(false) => "not available",
        None => "(unavailable)",
    }
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
    use crate::domain::{QueryResult, QuerySource};

    fn empty_query_result() -> QueryResult {
        QueryResult::success(String::new(), vec![], vec![], 0, QuerySource::Adhoc)
    }

    #[tokio::test]
    async fn fetch_diagnostics_core_reports_pragmas_without_quick_check() {
        let (_dir, dsn) =
            super::test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let snapshot = adapter.fetch_diagnostics_core(&dsn, true).await.unwrap();

        assert!(snapshot.db_file.is_ok());
        assert!(snapshot.sqlite_version.is_ok());
        assert!(snapshot.feature_summary.is_ok());
        assert!(snapshot.foreign_keys.is_ok());
        assert!(snapshot.journal_mode.is_ok());
        assert!(snapshot.query_only.is_ok());
        assert!(snapshot.busy_timeout.is_ok());
        assert!(snapshot.database_list.is_ok());
        assert!(matches!(snapshot.quick_check, DiagnosticField::Pending));
    }

    #[tokio::test]
    async fn fetch_quick_check_reports_integrity_summary() {
        let (_dir, dsn) =
            super::test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let quick_check = adapter.fetch_quick_check(&dsn, true).await;

        assert!(quick_check.is_ok());
        assert!(
            quick_check
                .ok_value()
                .is_some_and(|value| value.eq_ignore_ascii_case("ok"))
        );
    }

    #[test]
    fn scalar_field_maps_query_failure_to_error() {
        let field = scalar_field(Err(DbOperationError::QueryFailed("boom".to_string())));

        assert!(field.is_err());
    }

    #[test]
    fn scalar_field_maps_empty_rows_to_error() {
        let field = scalar_field(Ok(empty_query_result()));

        assert_eq!(field.err_message(), Some("empty result"));
    }

    #[test]
    fn quick_check_field_maps_empty_rows_to_error() {
        let field = quick_check_field(Ok(empty_query_result()));

        assert_eq!(field.err_message(), Some("quick_check: empty result"));
    }

    #[test]
    fn database_list_field_maps_empty_rows_to_error() {
        let field = database_list_field(Ok(empty_query_result()));

        assert_eq!(field.err_message(), Some("database_list: empty result"));
    }

    #[test]
    fn feature_summary_reports_compile_module_and_json_function_support() {
        let compile_options =
            FeatureProbe::Available(vec!["ENABLE_FTS5".to_string(), "ENABLE_RTREE".to_string()]);
        let module_list = FeatureProbe::Available(vec!["fts4".to_string()]);
        let function_list = FeatureProbe::Available(vec!["json_extract".to_string()]);

        let field = feature_summary_field(&compile_options, &module_list, &function_list);

        assert_eq!(
            field.ok_value(),
            Some("FTS5: available\nFTS4: available\nRTree: available\nJSON: available")
        );
    }

    #[test]
    fn feature_summary_keeps_unknown_json_availability_separate_from_absent_modules() {
        let compile_options = FeatureProbe::Available(Vec::new());
        let module_list = FeatureProbe::Unavailable;
        let function_list = FeatureProbe::Unavailable;

        let field = feature_summary_field(&compile_options, &module_list, &function_list);

        assert_eq!(
            field.ok_value(),
            Some(
                "FTS5: not available\nFTS4: not available\nRTree: not available\nJSON: (unavailable)"
            )
        );
    }

    #[test]
    fn feature_summary_is_unavailable_when_all_probes_are_unavailable() {
        let field = feature_summary_field(
            &FeatureProbe::Unavailable,
            &FeatureProbe::Unavailable,
            &FeatureProbe::Unavailable,
        );

        assert_eq!(field, DiagnosticField::Unavailable);
    }

    #[test]
    fn feature_probe_treats_pragma_without_columns_as_unavailable() {
        let field = feature_probe(Ok(empty_query_result()));

        assert_eq!(field, FeatureProbe::Unavailable);
    }

    #[test]
    fn format_on_off_maps_sqlite_integers() {
        assert_eq!(format_on_off("1"), "on");
        assert_eq!(format_on_off("0"), "off");
    }
}
