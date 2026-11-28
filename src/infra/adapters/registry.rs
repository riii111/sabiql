use crate::app::ports::outbound::{
    AccessMode, DbOperationError, MetadataProvider, MySqlConnectionProbe,
    MySqlConnectionProbeResult, QueryExecutor, SqliteDiagnosticsProvider,
};
use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{
    DatabaseMetadata, DiagnosticField, QueryResult, SqliteDiagnosticsSnapshot, Table,
    TableSignatureSnapshot, WriteExecutionResult,
};
use async_trait::async_trait;

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

macro_rules! delegate {
    ($registry:ident.$method:ident($endpoint:expr $(, $arg:expr)*)) => {
        match $endpoint {
            ConnectionEndpoint::PostgreSQL(endpoint) => {
                $registry.postgres.$method(endpoint $(, $arg)*).await
            }
            ConnectionEndpoint::SQLite(config) => $registry.sqlite.$method(config $(, $arg)*).await,
            ConnectionEndpoint::MySQL(config) => $registry.mysql.$method(config $(, $arg)*).await,
        }
    };
}

#[async_trait]
impl MetadataProvider for DbAdapterRegistry {
    async fn fetch_metadata(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<DatabaseMetadata, DbOperationError> {
        delegate!(self.fetch_metadata(endpoint))
    }

    async fn fetch_effective_user(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<Option<String>, DbOperationError> {
        match endpoint {
            ConnectionEndpoint::PostgreSQL(endpoint) => {
                self.postgres.fetch_effective_user(endpoint).await
            }
            ConnectionEndpoint::SQLite(_) => Ok(None),
            ConnectionEndpoint::MySQL(config) => self.mysql.fetch_effective_user(config).await,
        }
    }

    async fn fetch_table_detail(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        delegate!(self.fetch_table_detail(endpoint, schema, table))
    }

    async fn fetch_table_columns_and_fks(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        delegate!(self.fetch_table_columns_and_fks(endpoint, schema, table))
    }

    async fn fetch_table_columns_and_fks_batch(
        &self,
        endpoint: &ConnectionEndpoint,
        tables: &[(String, String)],
    ) -> Result<Vec<Table>, DbOperationError> {
        delegate!(self.fetch_table_columns_and_fks_batch(endpoint, tables))
    }

    async fn fetch_table_signatures(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        match endpoint {
            ConnectionEndpoint::PostgreSQL(config) => {
                self.postgres.fetch_table_signatures(config).await
            }
            ConnectionEndpoint::MySQL(config) => self.mysql.fetch_table_signatures(config).await,
            ConnectionEndpoint::SQLite(_) => Err(DbOperationError::UnsupportedOperation(
                "ER diagrams are not supported for SQLite".to_string(),
            )),
        }
    }
}

#[async_trait]
impl QueryExecutor for DbAdapterRegistry {
    async fn execute_preview(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, DbOperationError> {
        delegate!(self.execute_preview(endpoint, schema, table, limit, offset))
    }

    async fn execute_adhoc(
        &self,
        endpoint: &ConnectionEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        delegate!(self.execute_adhoc(endpoint, query, access_mode))
    }

    async fn execute_write(
        &self,
        endpoint: &ConnectionEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        delegate!(self.execute_write(endpoint, query, access_mode))
    }
}

#[async_trait]
impl MySqlConnectionProbe for DbAdapterRegistry {
    async fn probe(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<MySqlConnectionProbeResult, DbOperationError> {
        match endpoint {
            ConnectionEndpoint::MySQL(config) => self.mysql.probe(config).await,
            ConnectionEndpoint::PostgreSQL(_) | ConnectionEndpoint::SQLite(_) => {
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
        endpoint: &ConnectionEndpoint,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
        match endpoint {
            ConnectionEndpoint::SQLite(config) => self.sqlite.fetch_core_diagnostics(config).await,
            ConnectionEndpoint::PostgreSQL(_) | ConnectionEndpoint::MySQL(_) => {
                Err(DbOperationError::UnsupportedOperation(
                    "SQLite diagnostics require a SQLite connection".to_string(),
                ))
            }
        }
    }

    async fn fetch_quick_check(&self, endpoint: &ConnectionEndpoint) -> DiagnosticField {
        match endpoint {
            ConnectionEndpoint::SQLite(config) => self.sqlite.fetch_quick_check(config).await,
            ConnectionEndpoint::PostgreSQL(_) | ConnectionEndpoint::MySQL(_) => {
                DiagnosticField::Unavailable
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support;

    use rstest::rstest;

    use super::*;
    use crate::domain::SqlitePathError;
    use crate::domain::connection::{
        MySqlConnectionConfig, MySqlSslMode, PostgresConnectionConfig, PostgresEndpoint,
        SqliteConnectionConfig, SslMode,
    };

    fn sqlite_endpoint(path: &str) -> ConnectionEndpoint {
        ConnectionEndpoint::SQLite(SqliteConnectionConfig::new(path).unwrap())
    }

    fn make_sqlite_endpoint(sql: &str) -> (tempfile::TempDir, ConnectionEndpoint) {
        let (dir, config) = test_support::make_sqlite_db(sql);
        (dir, ConnectionEndpoint::SQLite(config))
    }

    fn postgres_endpoint() -> ConnectionEndpoint {
        ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Settings(PostgresConnectionConfig::new(
            "localhost",
            5432,
            "app",
            "",
            "",
            SslMode::Prefer,
        )))
    }

    fn mysql_endpoint(
        database: Option<&str>,
        username: &str,
        password: &str,
    ) -> ConnectionEndpoint {
        ConnectionEndpoint::MySQL(MySqlConnectionConfig::new(
            "localhost",
            3306,
            database.map(str::to_string),
            username,
            password,
            MySqlSslMode::Preferred,
        ))
    }

    #[tokio::test]
    async fn mysql_dsn_requires_a_selected_database_for_metadata() {
        let registry = DbAdapterRegistry::new();

        let result = registry.fetch_metadata(&mysql_endpoint(None, "", "")).await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }

    #[tokio::test]
    async fn sqlite_metadata_is_dispatched_to_sqlite_adapter() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let metadata = registry.fetch_metadata(&endpoint).await.unwrap();

        assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_metadata_for_missing_file_fails_without_creating_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let endpoint = sqlite_endpoint(path.to_str().unwrap());
        let registry = DbAdapterRegistry::new();

        let result = registry.fetch_metadata(&endpoint).await;

        assert!(matches!(
            result,
            Err(DbOperationError::SqlitePath(SqlitePathError::FileNotFound(file_path)))
                if file_path == path.display().to_string()
        ));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn sqlite_effective_user_dispatch_preserves_unknown_user() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let effective_user = registry.fetch_effective_user(&endpoint).await.unwrap();

        assert_eq!(effective_user, None);
    }

    #[tokio::test]
    async fn mysql_effective_user_dispatch_does_not_expose_password_on_validation_failure() {
        let registry = DbAdapterRegistry::new();

        let error = registry
            .fetch_effective_user(&mysql_endpoint(Some("app"), "app", "header-secret\u{1}"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, DbOperationError::ConnectionFailed(details) if !details.contains("header-secret"))
        );
    }

    #[tokio::test]
    async fn sqlite_table_signatures_are_unsupported() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();
        assert!(matches!(
            registry.fetch_table_signatures(&endpoint).await,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }

    #[tokio::test]
    async fn sqlite_table_detail_is_dispatched_to_sqlite_adapter() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let detail = registry
            .fetch_table_detail(&endpoint, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.schema, "main");
        assert_eq!(detail.name, "users");
    }

    #[tokio::test]
    async fn sqlite_query_execution_is_dispatched_to_sqlite_adapter() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let result = registry
            .execute_adhoc(&endpoint, "SELECT 1 AS value", AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.display_row_at(0), Some(vec!["1".to_string()]));
    }

    #[tokio::test]
    async fn sqlite_columns_request_is_dispatched_to_sqlite_adapter() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let detail = registry
            .fetch_table_columns_and_fks(&endpoint, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.schema, "main");
        assert_eq!(detail.name, "users");
    }

    #[tokio::test]
    async fn sqlite_diagnostics_are_dispatched_to_sqlite_adapter() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let snapshot = registry.fetch_core_diagnostics(&endpoint).await.unwrap();
        let quick_check = registry.fetch_quick_check(&endpoint).await;

        assert!(snapshot.sqlite_version.is_ok());
        assert!(
            quick_check
                .ok_value()
                .is_some_and(|value| value.eq_ignore_ascii_case("ok"))
        );
    }

    #[rstest]
    #[case::postgres(postgres_endpoint())]
    #[case::mysql(mysql_endpoint(Some("app"), "", ""))]
    #[tokio::test]
    async fn sqlite_core_diagnostics_reject_other_engines(#[case] endpoint: ConnectionEndpoint) {
        let registry = DbAdapterRegistry::new();

        let result = registry.fetch_core_diagnostics(&endpoint).await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }

    #[rstest]
    #[case::postgres(postgres_endpoint())]
    #[case::mysql(mysql_endpoint(Some("app"), "", ""))]
    #[tokio::test]
    async fn sqlite_quick_check_is_unavailable_for_other_engines(
        #[case] endpoint: ConnectionEndpoint,
    ) {
        let registry = DbAdapterRegistry::new();

        let quick_check = registry.fetch_quick_check(&endpoint).await;

        assert_eq!(quick_check, DiagnosticField::Unavailable);
    }

    #[tokio::test]
    async fn mysql_probe_dispatch_does_not_expose_password_on_validation_failure() {
        let registry = DbAdapterRegistry::new();

        let error = registry
            .probe(&mysql_endpoint(Some("app"), "app", "probe-secret\u{1}"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, DbOperationError::ConnectionFailed(details) if !details.contains("probe-secret"))
        );
    }

    #[rstest]
    #[case::postgres(postgres_endpoint())]
    #[case::sqlite(sqlite_endpoint("/tmp/app.db"))]
    #[tokio::test]
    async fn mysql_probe_rejects_other_engines(#[case] endpoint: ConnectionEndpoint) {
        let registry = DbAdapterRegistry::new();

        let result = registry.probe(&endpoint).await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }
}
