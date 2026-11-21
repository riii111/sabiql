use crate::app::ports::outbound::{
    AccessMode, DbOperationError, DdlGenerator, DsnBuilder, MetadataProvider, MySqlConnectionProbe,
    MySqlConnectionProbeResult, QueryExecutor, SqliteDiagnosticsProvider,
};
use crate::domain::connection::{ConnectionProfile, DatabaseType};
use crate::domain::{
    DatabaseMetadata, DiagnosticField, QueryResult, SqliteDiagnosticsSnapshot, Table,
    TableSignatureSnapshot, WriteExecutionResult,
};
use async_trait::async_trait;
use std::path::PathBuf;

use super::mysql::MySqlAdapter;
use super::postgres::PostgresAdapter;
use super::sqlite::SqliteAdapter;

pub struct DbAdapterRegistry {
    postgres: PostgresAdapter,
    sqlite: SqliteAdapter,
    mysql: MySqlAdapter,
}

impl DbAdapterRegistry {
    #[allow(
        clippy::new_without_default,
        reason = "new() is the only default construction API"
    )]
    pub fn new() -> Self {
        Self {
            postgres: PostgresAdapter::new(),
            sqlite: SqliteAdapter::new(),
            mysql: MySqlAdapter::new(),
        }
    }
}

impl DsnBuilder for DbAdapterRegistry {
    fn build_dsn(&self, profile: &ConnectionProfile) -> String {
        match profile.database_type() {
            DatabaseType::PostgreSQL => self.postgres.build_dsn(profile),
            DatabaseType::SQLite => {
                let path = &profile
                    .sqlite_config()
                    .expect("SQLite profile requires SQLite config")
                    .path();
                format!("sqlite://{path}")
            }
            DatabaseType::MySQL => self.mysql.build_dsn(profile),
        }
    }
}

#[async_trait]
impl MetadataProvider for DbAdapterRegistry {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => adapter.fetch_metadata(dsn).await,
            AdapterRoute::SQLite(adapter, dsn) => adapter.fetch_metadata(dsn).await,
            AdapterRoute::MySQL(adapter, dsn) => adapter.fetch_metadata(dsn).await,
        }
    }

    async fn fetch_effective_user(&self, dsn: &str) -> Result<Option<String>, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => adapter.fetch_effective_user(dsn).await,
            AdapterRoute::SQLite(..) => Ok(None),
            AdapterRoute::MySQL(adapter, dsn) => adapter.fetch_effective_user(dsn).await,
        }
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => {
                adapter.fetch_table_detail(dsn, schema, table).await
            }
            AdapterRoute::SQLite(adapter, dsn) => {
                adapter.fetch_table_detail(dsn, schema, table).await
            }
            AdapterRoute::MySQL(adapter, dsn) => {
                adapter.fetch_table_detail(dsn, schema, table).await
            }
        }
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => {
                adapter
                    .fetch_table_columns_and_fks(dsn, schema, table)
                    .await
            }
            AdapterRoute::SQLite(adapter, dsn) => {
                adapter
                    .fetch_table_columns_and_fks(dsn, schema, table)
                    .await
            }
            AdapterRoute::MySQL(adapter, dsn) => {
                adapter
                    .fetch_table_columns_and_fks(dsn, schema, table)
                    .await
            }
        }
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => adapter.fetch_table_signatures(dsn).await,
            AdapterRoute::SQLite(adapter, dsn) => adapter.fetch_table_signatures(dsn).await,
            AdapterRoute::MySQL(adapter, dsn) => adapter.fetch_table_signatures(dsn).await,
        }
    }
}

#[async_trait]
impl QueryExecutor for DbAdapterRegistry {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => {
                adapter
                    .execute_preview(dsn, schema, table, limit, offset)
                    .await
            }
            AdapterRoute::SQLite(adapter, dsn) => {
                adapter
                    .execute_preview(dsn, schema, table, limit, offset)
                    .await
            }
            AdapterRoute::MySQL(adapter, dsn) => {
                adapter
                    .execute_preview(dsn, schema, table, limit, offset)
                    .await
            }
        }
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => {
                adapter.execute_adhoc(dsn, query, access_mode).await
            }
            AdapterRoute::SQLite(adapter, dsn) => {
                adapter.execute_adhoc(dsn, query, access_mode).await
            }
            AdapterRoute::MySQL(adapter, dsn) => {
                adapter.execute_adhoc(dsn, query, access_mode).await
            }
        }
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => {
                adapter.execute_write(dsn, query, access_mode).await
            }
            AdapterRoute::SQLite(adapter, dsn) => {
                adapter.execute_write(dsn, query, access_mode).await
            }
            AdapterRoute::MySQL(adapter, dsn) => {
                adapter.execute_write(dsn, query, access_mode).await
            }
        }
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<PathBuf, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::PostgreSQL(adapter, dsn) => {
                adapter.export_to_csv(dsn, query, file_name).await
            }
            AdapterRoute::SQLite(adapter, dsn) => {
                adapter.export_to_csv(dsn, query, file_name).await
            }
            AdapterRoute::MySQL(adapter, dsn) => adapter.export_to_csv(dsn, query, file_name).await,
        }
    }
}

#[async_trait]
impl MySqlConnectionProbe for DbAdapterRegistry {
    async fn probe(&self, dsn: &str) -> Result<MySqlConnectionProbeResult, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::MySQL(adapter, dsn) => adapter.probe(dsn).await,
            AdapterRoute::PostgreSQL(..) | AdapterRoute::SQLite(..) => {
                Err(DbOperationError::UnsupportedOperation(
                    "MySQL connection probe requires a MySQL connection".to_string(),
                ))
            }
        }
    }
}

#[async_trait]
impl SqliteDiagnosticsProvider for DbAdapterRegistry {
    async fn fetch_core_diagnostics(
        &self,
        dsn: &str,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
        match self.route(dsn)? {
            AdapterRoute::SQLite(adapter, dsn) => adapter.fetch_core_diagnostics(dsn).await,
            AdapterRoute::PostgreSQL(..) | AdapterRoute::MySQL(..) => {
                Err(DbOperationError::UnsupportedOperation(
                    "SQLite diagnostics require a SQLite connection".to_string(),
                ))
            }
        }
    }

    async fn fetch_quick_check(&self, dsn: &str) -> DiagnosticField {
        match self.route(dsn) {
            Ok(AdapterRoute::SQLite(adapter, dsn)) => adapter.fetch_quick_check(dsn).await,
            Ok(AdapterRoute::PostgreSQL(..) | AdapterRoute::MySQL(..)) => {
                DiagnosticField::Unavailable
            }
            Err(error) => DiagnosticField::err(error.masked_details()),
        }
    }
}

impl DdlGenerator for DbAdapterRegistry {
    fn generate_ddl(&self, database_type: DatabaseType, table: &Table) -> String {
        match database_type {
            DatabaseType::PostgreSQL => self.postgres.generate_ddl(table),
            DatabaseType::SQLite => self.sqlite.generate_ddl(table),
            DatabaseType::MySQL => self.mysql.generate_ddl(table),
        }
    }
}

enum AdapterRoute<'a> {
    PostgreSQL(&'a PostgresAdapter, &'a str),
    SQLite(&'a SqliteAdapter, &'a str),
    MySQL(&'a MySqlAdapter, &'a str),
}

impl DbAdapterRegistry {
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its adapter so a new engine fails to compile instead of silently reaching another engine's adapter."
    )]
    fn route<'a>(&'a self, dsn: &'a str) -> Result<AdapterRoute<'a>, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => Ok(AdapterRoute::PostgreSQL(&self.postgres, dsn)),
            DatabaseType::SQLite => Ok(AdapterRoute::SQLite(&self.sqlite, dsn)),
            DatabaseType::MySQL => Ok(AdapterRoute::MySQL(&self.mysql, dsn)),
        }
    }

    fn db_type_from_dsn(dsn: &str) -> Result<DatabaseType, DbOperationError> {
        if dsn.starts_with("sqlite://") {
            return Ok(DatabaseType::SQLite);
        }
        if dsn.starts_with("mysql://") {
            return Ok(DatabaseType::MySQL);
        }
        if dsn.starts_with("postgres://") || is_postgres_conninfo_dsn(dsn) {
            return Ok(DatabaseType::PostgreSQL);
        }
        Err(DbOperationError::ConnectionFailed(format!(
            "Unsupported database DSN scheme: {dsn}"
        )))
    }
}

fn is_postgres_conninfo_dsn(dsn: &str) -> bool {
    let Some((key, _)) = dsn.trim_start().split_once('=') else {
        return false;
    };
    matches!(
        key.trim(),
        "host" | "hostaddr" | "port" | "dbname" | "user" | "password" | "sslmode" | "service"
    )
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support;

    use rstest::rstest;

    use super::*;
    use crate::domain::connection::SslMode;
    use crate::domain::{Column, ColumnAttributes, SqlitePathError};

    fn make_table() -> Table {
        Table {
            schema: "main".to_string(),
            name: "users".to_string(),
            columns: vec![Column {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                default: None,
                attributes: ColumnAttributes::from_parts(false, true, false),
                comment: None,
                ordinal_position: 1,
                character_set_name: None,
                collation_name: None,
                generation_expression: None,
                generation_kind: None,
            }],
            primary_key: Some(vec!["id".to_string()]),
            ..test_support::minimal_table("", "")
        }
    }

    #[test]
    fn builds_postgres_dsn_from_postgres_profile() {
        let registry = DbAdapterRegistry::new();
        let profile = ConnectionProfile::new_postgres(
            "Test",
            "localhost",
            5432,
            "db",
            "user",
            "pass",
            SslMode::Prefer,
        )
        .unwrap();

        let dsn = registry.build_dsn(&profile);

        assert!(dsn.starts_with("host='localhost'"));
    }

    #[test]
    fn builds_sqlite_dsn_from_sqlite_profile() {
        let registry = DbAdapterRegistry::new();
        let profile = ConnectionProfile::new_sqlite("Local", "/tmp/app.db").unwrap();

        let dsn = registry.build_dsn(&profile);

        assert_eq!(dsn, "sqlite:///tmp/app.db");
    }

    #[test]
    fn mysql_registry_preserves_source_ddl() {
        let registry = DbAdapterRegistry::new();
        let source_ddl = "CREATE TABLE `users` (`id` INT)".to_string();
        let mut table = make_table();
        table.source_ddl = Some(source_ddl.clone());

        assert_eq!(
            registry.generate_ddl(DatabaseType::MySQL, &table),
            source_ddl
        );
    }

    #[tokio::test]
    async fn mysql_dsn_requires_a_selected_database_for_metadata() {
        let registry = DbAdapterRegistry::new();

        let result = registry.fetch_metadata("mysql://localhost").await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }

    #[rstest]
    #[case::sqlite_url("sqlite:///tmp/app.db", Some(DatabaseType::SQLite))]
    #[case::mysql_url("mysql://localhost/db", Some(DatabaseType::MySQL))]
    #[case::postgres_url("postgres://localhost/db", Some(DatabaseType::PostgreSQL))]
    #[case::postgres_conninfo("host='localhost' dbname='db'", Some(DatabaseType::PostgreSQL))]
    #[case::unsupported_scheme("redis://localhost", None)]
    fn classifies_named_dsn_inputs(#[case] dsn: &str, #[case] expected: Option<DatabaseType>) {
        assert_eq!(DbAdapterRegistry::db_type_from_dsn(dsn).ok(), expected);
    }

    #[tokio::test]
    async fn sqlite_metadata_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let metadata = registry.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_metadata_for_missing_file_fails_without_creating_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let dsn = format!("sqlite://{}", path.display());
        let registry = DbAdapterRegistry::new();

        let result = registry.fetch_metadata(&dsn).await;

        assert!(matches!(
            result,
            Err(DbOperationError::SqlitePath(SqlitePathError::FileNotFound(file_path)))
                if file_path == path.display().to_string()
        ));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn sqlite_effective_user_dispatch_preserves_unknown_user() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let effective_user = registry.fetch_effective_user(&dsn).await.unwrap();

        assert_eq!(effective_user, None);
    }

    #[tokio::test]
    async fn mysql_effective_user_dispatch_does_not_expose_password_on_validation_failure() {
        let registry = DbAdapterRegistry::new();

        let error = registry
            .fetch_effective_user("mysql://app:header-secret%01@localhost/app")
            .await
            .unwrap_err();

        assert!(
            matches!(error, DbOperationError::ConnectionFailed(details) if !details.contains("header-secret"))
        );
    }

    #[tokio::test]
    async fn sqlite_table_signatures_are_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let signatures = registry.fetch_table_signatures(&dsn).await.unwrap();

        assert_eq!(signatures.signatures[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_table_detail_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let detail = registry
            .fetch_table_detail(&dsn, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.schema, "main");
        assert_eq!(detail.name, "users");
    }

    #[tokio::test]
    async fn sqlite_query_execution_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let result = registry
            .execute_adhoc(&dsn, "SELECT 1 AS value", AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.display_row_at(0), Some(vec!["1".to_string()]));
    }

    #[tokio::test]
    async fn sqlite_columns_request_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let detail = registry
            .fetch_table_columns_and_fks(&dsn, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.schema, "main");
        assert_eq!(detail.name, "users");
    }

    #[tokio::test]
    async fn sqlite_diagnostics_are_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let snapshot = registry.fetch_core_diagnostics(&dsn).await.unwrap();
        let quick_check = registry.fetch_quick_check(&dsn).await;

        assert!(snapshot.sqlite_version.is_ok());
        assert!(
            quick_check
                .ok_value()
                .is_some_and(|value| value.eq_ignore_ascii_case("ok"))
        );
    }

    #[rstest]
    #[case::postgres("host='localhost' dbname='app'")]
    #[case::mysql("mysql://localhost/app")]
    #[tokio::test]
    async fn sqlite_core_diagnostics_reject_other_engines(#[case] dsn: &str) {
        let registry = DbAdapterRegistry::new();

        let result = registry.fetch_core_diagnostics(dsn).await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }

    #[rstest]
    #[case::postgres("host='localhost' dbname='app'")]
    #[case::mysql("mysql://localhost/app")]
    #[tokio::test]
    async fn sqlite_quick_check_is_unavailable_for_other_engines(#[case] dsn: &str) {
        let registry = DbAdapterRegistry::new();

        let quick_check = registry.fetch_quick_check(dsn).await;

        assert_eq!(quick_check, DiagnosticField::Unavailable);
    }

    #[tokio::test]
    async fn mysql_probe_dispatch_does_not_expose_password_on_validation_failure() {
        let registry = DbAdapterRegistry::new();

        let error = registry
            .probe("mysql://app:probe-secret%01@localhost/app")
            .await
            .unwrap_err();

        assert!(
            matches!(error, DbOperationError::ConnectionFailed(details) if !details.contains("probe-secret"))
        );
    }

    #[rstest]
    #[case::postgres("host='localhost' dbname='app'")]
    #[case::sqlite("sqlite:///tmp/app.db")]
    #[tokio::test]
    async fn mysql_probe_rejects_other_engines(#[case] dsn: &str) {
        let registry = DbAdapterRegistry::new();

        let result = registry.probe(dsn).await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }
}
