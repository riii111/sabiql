use crate::app::ports::outbound::{
    AccessMode, DbOperationError, DdlGenerator, DsnBuilder, MetadataProvider, MySqlConnectionProbe,
    MySqlConnectionProbeResult, QueryExecutor, SqliteDiagnosticsProvider,
};
use crate::domain::connection::{
    ConnectionEndpoint, DatabaseType, MySqlConnectionConfig, PostgresEndpoint,
    SqliteConnectionConfig,
};
use crate::domain::{
    DatabaseMetadata, DiagnosticField, QueryResult, SqliteDiagnosticsSnapshot, Table,
    TableSignatureSnapshot, WriteExecutionResult,
};
use async_trait::async_trait;
use std::path::PathBuf;

use super::mysql::{MySqlAdapter, build_mysql_dsn};
use super::postgres::{PostgresAdapter, postgres_conninfo};
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
    fn build_dsn(&self, endpoint: &ConnectionEndpoint) -> String {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(_, endpoint) => postgres_conninfo(endpoint),
            AdapterRoute::SQLite(_, config) => sqlite_dsn(config),
            AdapterRoute::MySQL(_, config) => build_mysql_dsn(config),
        }
    }
}

#[async_trait]
impl MetadataProvider for DbAdapterRegistry {
    async fn fetch_metadata(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<DatabaseMetadata, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => adapter.fetch_metadata(endpoint).await,
            AdapterRoute::SQLite(adapter, config) => adapter.fetch_metadata(config).await,
            AdapterRoute::MySQL(adapter, config) => {
                adapter.fetch_metadata(&build_mysql_dsn(config)).await
            }
        }
    }

    async fn fetch_effective_user(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<Option<String>, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter.fetch_effective_user(endpoint).await
            }
            AdapterRoute::SQLite(..) => Ok(None),
            AdapterRoute::MySQL(adapter, config) => {
                adapter.fetch_effective_user(&build_mysql_dsn(config)).await
            }
        }
    }

    async fn fetch_table_detail(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter.fetch_table_detail(endpoint, schema, table).await
            }
            AdapterRoute::SQLite(adapter, config) => {
                adapter.fetch_table_detail(config, schema, table).await
            }
            AdapterRoute::MySQL(adapter, config) => {
                adapter
                    .fetch_table_detail(&build_mysql_dsn(config), schema, table)
                    .await
            }
        }
    }

    async fn fetch_table_columns_and_fks(
        &self,
        endpoint: &ConnectionEndpoint,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter
                    .fetch_table_columns_and_fks(endpoint, schema, table)
                    .await
            }
            AdapterRoute::SQLite(adapter, config) => {
                adapter
                    .fetch_table_columns_and_fks(config, schema, table)
                    .await
            }
            AdapterRoute::MySQL(adapter, config) => {
                adapter
                    .fetch_table_columns_and_fks(&build_mysql_dsn(config), schema, table)
                    .await
            }
        }
    }

    async fn fetch_table_signatures(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter.fetch_table_signatures(endpoint).await
            }
            AdapterRoute::SQLite(adapter, config) => adapter.fetch_table_signatures(config).await,
            AdapterRoute::MySQL(adapter, config) => {
                adapter
                    .fetch_table_signatures(&build_mysql_dsn(config))
                    .await
            }
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
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter
                    .execute_preview(endpoint, schema, table, limit, offset)
                    .await
            }
            AdapterRoute::SQLite(adapter, config) => {
                adapter
                    .execute_preview(config, schema, table, limit, offset)
                    .await
            }
            AdapterRoute::MySQL(adapter, config) => {
                adapter
                    .execute_preview(&build_mysql_dsn(config), schema, table, limit, offset)
                    .await
            }
        }
    }

    async fn execute_adhoc(
        &self,
        endpoint: &ConnectionEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter.execute_adhoc(endpoint, query, access_mode).await
            }
            AdapterRoute::SQLite(adapter, config) => {
                adapter.execute_adhoc(config, query, access_mode).await
            }
            AdapterRoute::MySQL(adapter, config) => {
                adapter
                    .execute_adhoc(&build_mysql_dsn(config), query, access_mode)
                    .await
            }
        }
    }

    async fn execute_write(
        &self,
        endpoint: &ConnectionEndpoint,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter.execute_write(endpoint, query, access_mode).await
            }
            AdapterRoute::SQLite(adapter, config) => {
                adapter.execute_write(config, query, access_mode).await
            }
            AdapterRoute::MySQL(adapter, config) => {
                adapter
                    .execute_write(&build_mysql_dsn(config), query, access_mode)
                    .await
            }
        }
    }

    async fn export_to_csv(
        &self,
        endpoint: &ConnectionEndpoint,
        query: &str,
        file_name: &str,
    ) -> Result<PathBuf, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::PostgreSQL(adapter, endpoint) => {
                adapter.export_to_csv(endpoint, query, file_name).await
            }
            AdapterRoute::SQLite(adapter, config) => {
                adapter.export_to_csv(config, query, file_name).await
            }
            AdapterRoute::MySQL(adapter, config) => {
                adapter
                    .export_to_csv(&build_mysql_dsn(config), query, file_name)
                    .await
            }
        }
    }
}

#[async_trait]
impl MySqlConnectionProbe for DbAdapterRegistry {
    async fn probe(
        &self,
        endpoint: &ConnectionEndpoint,
    ) -> Result<MySqlConnectionProbeResult, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::MySQL(adapter, config) => adapter.probe(&build_mysql_dsn(config)).await,
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
        endpoint: &ConnectionEndpoint,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
        match self.route(endpoint) {
            AdapterRoute::SQLite(adapter, config) => adapter.fetch_core_diagnostics(config).await,
            AdapterRoute::PostgreSQL(..) | AdapterRoute::MySQL(..) => {
                Err(DbOperationError::UnsupportedOperation(
                    "SQLite diagnostics require a SQLite connection".to_string(),
                ))
            }
        }
    }

    async fn fetch_quick_check(&self, endpoint: &ConnectionEndpoint) -> DiagnosticField {
        match self.route(endpoint) {
            AdapterRoute::SQLite(adapter, config) => adapter.fetch_quick_check(config).await,
            AdapterRoute::PostgreSQL(..) | AdapterRoute::MySQL(..) => DiagnosticField::Unavailable,
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
    PostgreSQL(&'a PostgresAdapter, &'a PostgresEndpoint),
    SQLite(&'a SqliteAdapter, &'a SqliteConnectionConfig),
    MySQL(&'a MySqlAdapter, &'a MySqlConnectionConfig),
}

impl DbAdapterRegistry {
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its adapter so a new engine fails to compile instead of silently reaching another engine's adapter."
    )]
    fn route<'a>(&'a self, endpoint: &'a ConnectionEndpoint) -> AdapterRoute<'a> {
        match endpoint {
            ConnectionEndpoint::PostgreSQL(endpoint) => {
                AdapterRoute::PostgreSQL(&self.postgres, endpoint)
            }
            ConnectionEndpoint::SQLite(config) => AdapterRoute::SQLite(&self.sqlite, config),
            ConnectionEndpoint::MySQL(config) => AdapterRoute::MySQL(&self.mysql, config),
        }
    }
}

fn sqlite_dsn(config: &SqliteConnectionConfig) -> String {
    format!("sqlite://{}", config.path())
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support;

    use rstest::rstest;

    use super::*;
    use crate::domain::connection::{
        ConnectionProfile, MySqlSslMode, PostgresConnectionConfig, SslMode,
    };
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

        let dsn = registry.build_dsn(&ConnectionEndpoint::from(&profile.config));

        assert!(dsn.starts_with("host='localhost'"));
    }

    #[test]
    fn builds_sqlite_dsn_from_sqlite_profile() {
        let registry = DbAdapterRegistry::new();
        let profile = ConnectionProfile::new_sqlite("Local", "/tmp/app.db").unwrap();

        let dsn = registry.build_dsn(&ConnectionEndpoint::from(&profile.config));

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
    async fn sqlite_table_signatures_are_dispatched_to_sqlite_adapter() {
        let (_dir, endpoint) = make_sqlite_endpoint("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new();

        let signatures = registry.fetch_table_signatures(&endpoint).await.unwrap();

        assert_eq!(signatures.signatures[0].qualified_name(), "main.users");
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
