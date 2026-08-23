use std::path::PathBuf;
use std::sync::Arc;

use crate::app::ports::outbound::{
    AccessMode, DbOperationError, DdlGenerator, DsnBuilder, MetadataProvider, MySqlConnectionProbe,
    MySqlConnectionProbeResult, QueryExecutor, SqlDialect, SqliteDiagnosticsProvider,
};
use crate::domain::connection::{ConnectionProfile, DatabaseType};
use crate::domain::{
    DatabaseMetadata, DiagnosticField, QueryResult, QueryValue, SqliteDiagnosticsSnapshot, Table,
    TableSignatureSnapshot, WriteExecutionResult,
};
use async_trait::async_trait;

use super::mysql::MySqlAdapter;
use super::postgres::PostgresAdapter;
use super::sqlite::SqliteAdapter;

pub struct DbAdapterRegistry {
    postgres: Arc<PostgresAdapter>,
    sqlite: Arc<SqliteAdapter>,
    mysql: Arc<MySqlAdapter>,
}

impl DbAdapterRegistry {
    pub fn new(postgres: Arc<PostgresAdapter>) -> Self {
        Self {
            postgres,
            sqlite: Arc::new(SqliteAdapter::new()),
            mysql: Arc::new(MySqlAdapter::new()),
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

    fn metadata_provider(&self, dsn: &str) -> Result<&dyn MetadataProvider, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => Ok(self.postgres.as_ref()),
            DatabaseType::SQLite => Ok(self.sqlite.as_ref()),
            DatabaseType::MySQL => Ok(self.mysql.as_ref()),
        }
    }

    fn query_executor(&self, dsn: &str) -> Result<&dyn QueryExecutor, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => Ok(self.postgres.as_ref()),
            DatabaseType::SQLite => Ok(self.sqlite.as_ref()),
            DatabaseType::MySQL => Ok(self.mysql.as_ref()),
        }
    }

    fn ddl_generator(&self, database_type: DatabaseType) -> &dyn DdlGenerator {
        match database_type {
            DatabaseType::PostgreSQL => self.postgres.as_ref(),
            DatabaseType::SQLite => self.sqlite.as_ref(),
            DatabaseType::MySQL => self.mysql.as_ref(),
        }
    }

    fn sql_dialect(&self, database_type: DatabaseType) -> &dyn SqlDialect {
        match database_type {
            DatabaseType::PostgreSQL => self.postgres.as_ref(),
            DatabaseType::SQLite => self.sqlite.as_ref(),
            DatabaseType::MySQL => self.mysql.as_ref(),
        }
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
        self.metadata_provider(dsn)?.fetch_metadata(dsn).await
    }

    async fn fetch_effective_user(&self, dsn: &str) -> Result<Option<String>, DbOperationError> {
        self.metadata_provider(dsn)?.fetch_effective_user(dsn).await
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        self.metadata_provider(dsn)?
            .fetch_table_detail(dsn, schema, table)
            .await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        self.metadata_provider(dsn)?
            .fetch_table_columns_and_fks(dsn, schema, table)
            .await
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        self.metadata_provider(dsn)?
            .fetch_table_signatures(dsn)
            .await
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
        self.query_executor(dsn)?
            .execute_preview(dsn, schema, table, limit, offset)
            .await
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        self.query_executor(dsn)?
            .execute_adhoc(dsn, query, access_mode)
            .await
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        self.query_executor(dsn)?
            .execute_write(dsn, query, access_mode)
            .await
    }

    async fn count_query_rows(&self, dsn: &str, query: &str) -> Result<usize, DbOperationError> {
        self.query_executor(dsn)?.count_query_rows(dsn, query).await
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<PathBuf, DbOperationError> {
        self.query_executor(dsn)?
            .export_to_csv(dsn, query, file_name)
            .await
    }
}

impl DdlGenerator for DbAdapterRegistry {
    fn generate_ddl(&self, database_type: DatabaseType, table: &Table) -> String {
        self.ddl_generator(database_type)
            .generate_ddl(database_type, table)
    }
}

impl SqlDialect for DbAdapterRegistry {
    fn build_explain_sql(&self, database_type: DatabaseType, query: &str) -> Option<String> {
        self.sql_dialect(database_type)
            .build_explain_sql(database_type, query)
    }

    fn build_explain_analyze_sql(
        &self,
        database_type: DatabaseType,
        query: &str,
    ) -> Option<String> {
        self.sql_dialect(database_type)
            .build_explain_analyze_sql(database_type, query)
    }

    fn build_update_sql(
        &self,
        database_type: DatabaseType,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        self.sql_dialect(database_type).build_update_sql(
            database_type,
            schema,
            table,
            column,
            new_value,
            pk_pairs,
        )
    }

    fn build_bulk_delete_sql(
        &self,
        database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        self.sql_dialect(database_type).build_bulk_delete_sql(
            database_type,
            schema,
            table,
            pk_pairs_per_row,
        )
    }
}

#[async_trait]
impl MySqlConnectionProbe for DbAdapterRegistry {
    async fn probe(&self, dsn: &str) -> Result<MySqlConnectionProbeResult, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::MySQL => self.mysql.probe(dsn).await,
            DatabaseType::PostgreSQL | DatabaseType::SQLite => {
                Err(DbOperationError::UnsupportedOperation(
                    "Connection probing is only implemented for MySQL".to_string(),
                ))
            }
        }
    }
}

#[async_trait]
impl SqliteDiagnosticsProvider for DbAdapterRegistry {
    async fn fetch_diagnostics_core(
        &self,
        dsn: &str,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL | DatabaseType::MySQL => {
                Err(DbOperationError::ConnectionFailed(
                    "SQLite diagnostics are unavailable for non-SQLite connections".to_string(),
                ))
            }
            DatabaseType::SQLite => self.sqlite.fetch_diagnostics_core(dsn).await,
        }
    }

    async fn fetch_quick_check(&self, dsn: &str) -> DiagnosticField {
        match Self::db_type_from_dsn(dsn) {
            Ok(DatabaseType::SQLite) => self.sqlite.fetch_quick_check(dsn).await,
            Ok(DatabaseType::PostgreSQL | DatabaseType::MySQL) | Err(_) => DiagnosticField::err(
                "SQLite diagnostics are unavailable for non-SQLite connections",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support;

    use super::*;
    use crate::domain::connection::SslMode;
    use crate::domain::{Column, ColumnAttributes};

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
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));
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
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));
        let profile = ConnectionProfile::new_sqlite("Local", "/tmp/app.db").unwrap();

        let dsn = registry.build_dsn(&profile);

        assert_eq!(dsn, "sqlite:///tmp/app.db");
    }

    #[test]
    fn postgres_sql_generation_keeps_schema_qualified_sql() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));
        let rows = vec![vec![("id".to_string(), QueryValue::text("1"))]];

        let update_sql = registry.build_update_sql(
            DatabaseType::PostgreSQL,
            "public",
            "users",
            "name",
            &QueryValue::text("Bob"),
            &[("id".into(), QueryValue::text("1"))],
        );
        let delete_sql =
            registry.build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "users", &rows);
        let ddl = registry.generate_ddl(DatabaseType::PostgreSQL, &make_table());

        assert_eq!(
            update_sql,
            "UPDATE \"public\".\"users\"\nSET \"name\" = 'Bob'\nWHERE \"id\" = '1';"
        );
        assert_eq!(
            delete_sql,
            "DELETE FROM \"public\".\"users\"\nWHERE \"id\" = '1';"
        );
        assert!(ddl.contains("CREATE TABLE \"main\".\"users\""));
    }

    #[test]
    fn sqlite_sql_generation_omits_schema_qualification() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));
        let rows = vec![vec![("id".to_string(), QueryValue::text("1"))]];

        let update_sql = registry.build_update_sql(
            DatabaseType::SQLite,
            "main",
            "users",
            "name",
            &QueryValue::text("Bob"),
            &[("id".into(), QueryValue::text("1"))],
        );
        let delete_sql =
            registry.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);
        let ddl = registry.generate_ddl(DatabaseType::SQLite, &make_table());

        assert_eq!(
            update_sql,
            "UPDATE \"users\"\nSET \"name\" = 'Bob'\nWHERE \"id\" = '1';"
        );
        assert_eq!(delete_sql, "DELETE FROM \"users\"\nWHERE \"id\" = '1';");
        assert!(ddl.contains("CREATE TABLE \"users\""));
        assert!(!ddl.contains("\"main\".\"users\""));
    }

    #[test]
    fn postgres_explain_generation_uses_postgres_dialect() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        assert_eq!(
            registry.build_explain_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN SELECT 1".to_string())
        );
        assert_eq!(
            registry.build_explain_analyze_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE SELECT 1".to_string())
        );
    }

    #[test]
    fn sqlite_explain_generation_uses_query_plan() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        assert_eq!(
            registry.build_explain_sql(DatabaseType::SQLite, "SELECT 1"),
            Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
        );
        assert_eq!(
            registry.build_explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"),
            None
        );
        assert_eq!(
            registry.build_explain_sql(DatabaseType::SQLite, "DELETE FROM users"),
            Some("EXPLAIN QUERY PLAN DELETE FROM users".to_string())
        );
    }

    #[test]
    fn mysql_registry_dispatches_ddl_and_dialect_to_mysql_adapter() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));
        let source_ddl = "CREATE TABLE `users` (`id` INT)".to_string();
        let mut table = make_table();
        table.source_ddl = Some(source_ddl.clone());

        assert_eq!(
            registry.generate_ddl(DatabaseType::MySQL, &table),
            source_ddl
        );
        assert_eq!(
            registry.build_explain_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN FORMAT=TREE SELECT 1".to_string())
        );
        assert_eq!(
            registry.build_explain_analyze_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE FORMAT=TREE SELECT 1".to_string())
        );
    }

    #[tokio::test]
    async fn mysql_dsn_requires_a_selected_database_for_metadata() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let result = registry.fetch_metadata("mysql://localhost").await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn mysql_dsn_scheme_is_recognized() {
        assert_eq!(
            DbAdapterRegistry::db_type_from_dsn("mysql://localhost/db").unwrap(),
            DatabaseType::MySQL
        );
    }

    #[tokio::test]
    async fn sqlite_metadata_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let metadata = registry.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_effective_user_dispatch_preserves_unknown_user() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let effective_user = registry.fetch_effective_user(&dsn).await.unwrap();

        assert_eq!(effective_user, None);
    }

    #[tokio::test]
    async fn mysql_effective_user_dispatch_does_not_expose_password_on_validation_failure() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

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
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let signatures = registry.fetch_table_signatures(&dsn).await.unwrap();

        assert_eq!(signatures.signatures[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_table_detail_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

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
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

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
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let detail = registry
            .fetch_table_columns_and_fks(&dsn, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.schema, "main");
        assert_eq!(detail.name, "users");
    }

    #[tokio::test]
    async fn postgres_dsn_is_rejected() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let result = registry
            .fetch_diagnostics_core("postgres://localhost/db")
            .await;

        assert!(matches!(result, Err(DbOperationError::ConnectionFailed(_))));
    }
}
