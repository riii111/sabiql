use std::path::Path;
use std::sync::Arc;

use crate::app::ports::outbound::{
    DbOperationError, DdlGenerator, DsnBuilder, MetadataProvider, QueryExecutor, SqlDialect,
    SqliteDiagnosticsProvider,
};
use crate::domain::connection::{ConnectionProfile, DatabaseType};
use crate::domain::{
    DatabaseMetadata, DiagnosticField, QueryResult, QueryValue, SqliteDiagnosticsSnapshot, Table,
    TableSignature, WriteExecutionResult,
};
use async_trait::async_trait;

use super::postgres::PostgresAdapter;
use super::sqlite::SqliteAdapter;

pub struct DbAdapterRegistry {
    postgres: Arc<PostgresAdapter>,
    sqlite: Arc<SqliteAdapter>,
}

impl DbAdapterRegistry {
    pub fn new(postgres: Arc<PostgresAdapter>) -> Self {
        Self {
            postgres,
            sqlite: Arc::new(SqliteAdapter::new()),
        }
    }

    fn db_type_from_dsn(dsn: &str) -> Result<DatabaseType, DbOperationError> {
        if dsn.starts_with("sqlite://") {
            return Ok(DatabaseType::SQLite);
        }
        if dsn.starts_with("postgres://") || dsn.starts_with("service=") {
            return Ok(DatabaseType::PostgreSQL);
        }
        Err(DbOperationError::ConnectionFailed(format!(
            "Unsupported database DSN scheme: {dsn}"
        )))
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
        }
    }
}

#[async_trait]
impl MetadataProvider for DbAdapterRegistry {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => self.postgres.fetch_metadata(dsn).await,
            DatabaseType::SQLite => self.sqlite.fetch_metadata(dsn).await,
        }
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => self.postgres.fetch_table_detail(dsn, schema, table).await,
            DatabaseType::SQLite => self.sqlite.fetch_table_detail(dsn, schema, table).await,
        }
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => {
                self.postgres
                    .fetch_table_columns_and_fks(dsn, schema, table)
                    .await
            }
            DatabaseType::SQLite => {
                self.sqlite
                    .fetch_table_columns_and_fks(dsn, schema, table)
                    .await
            }
        }
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => self.postgres.fetch_table_signatures(dsn).await,
            DatabaseType::SQLite => self.sqlite.fetch_table_signatures(dsn).await,
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
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => {
                self.postgres
                    .execute_preview(dsn, schema, table, limit, offset, read_only)
                    .await
            }
            DatabaseType::SQLite => {
                self.sqlite
                    .execute_preview(dsn, schema, table, limit, offset, read_only)
                    .await
            }
        }
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => self.postgres.execute_adhoc(dsn, query, read_only).await,
            DatabaseType::SQLite => self.sqlite.execute_adhoc(dsn, query, read_only).await,
        }
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => self.postgres.execute_write(dsn, query, read_only).await,
            DatabaseType::SQLite => self.sqlite.execute_write(dsn, query, read_only).await,
        }
    }

    async fn count_query_rows(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => self.postgres.count_query_rows(dsn, query, read_only).await,
            DatabaseType::SQLite => self.sqlite.count_query_rows(dsn, query, read_only).await,
        }
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        path: &Path,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => {
                self.postgres
                    .export_to_csv(dsn, query, path, read_only)
                    .await
            }
            DatabaseType::SQLite => self.sqlite.export_to_csv(dsn, query, path, read_only).await,
        }
    }
}

impl DdlGenerator for DbAdapterRegistry {
    fn generate_ddl(&self, database_type: DatabaseType, table: &Table) -> String {
        match database_type {
            DatabaseType::PostgreSQL => self.postgres.generate_ddl(database_type, table),
            DatabaseType::SQLite => self.sqlite.generate_ddl(database_type, table),
        }
    }
}

impl SqlDialect for DbAdapterRegistry {
    fn build_explain_sql(&self, database_type: DatabaseType, query: &str) -> Option<String> {
        match database_type {
            DatabaseType::PostgreSQL => self.postgres.build_explain_sql(database_type, query),
            DatabaseType::SQLite => self.sqlite.build_explain_sql(database_type, query),
        }
    }

    fn build_explain_analyze_sql(
        &self,
        database_type: DatabaseType,
        query: &str,
    ) -> Option<String> {
        match database_type {
            DatabaseType::PostgreSQL => self
                .postgres
                .build_explain_analyze_sql(database_type, query),
            DatabaseType::SQLite => self.sqlite.build_explain_analyze_sql(database_type, query),
        }
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
        match database_type {
            DatabaseType::PostgreSQL => self.postgres.build_update_sql(
                database_type,
                schema,
                table,
                column,
                new_value,
                pk_pairs,
            ),
            DatabaseType::SQLite => self.sqlite.build_update_sql(
                database_type,
                schema,
                table,
                column,
                new_value,
                pk_pairs,
            ),
        }
    }

    fn build_bulk_delete_sql(
        &self,
        database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        match database_type {
            DatabaseType::PostgreSQL => {
                self.postgres
                    .build_bulk_delete_sql(database_type, schema, table, pk_pairs_per_row)
            }
            DatabaseType::SQLite => {
                self.sqlite
                    .build_bulk_delete_sql(database_type, schema, table, pk_pairs_per_row)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::test_support::make_sqlite_db;
    use crate::domain::connection::SslMode;
    use crate::domain::{Column, ColumnAttributes, QueryValue};

    fn make_table() -> Table {
        Table {
            schema: "main".to_string(),
            name: "users".to_string(),
            owner: None,
            columns: vec![Column {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                default: None,
                attributes: ColumnAttributes::from_parts(false, true, false),
                comment: None,
                ordinal_position: 1,
            }],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            indexes: vec![],
            rls: None,
            triggers: vec![],
            row_count_estimate: None,
            comment: None,
            source_ddl: None,
            ..Default::default()
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

        assert!(dsn.starts_with("postgres://"));
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
            None
        );
    }

    #[tokio::test]
    async fn unknown_dsn_scheme_is_rejected() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let result = registry.fetch_metadata("mysql://localhost/db").await;

        assert!(matches!(result, Err(DbOperationError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn sqlite_metadata_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let metadata = registry.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_table_signatures_are_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let signatures = registry.fetch_table_signatures(&dsn).await.unwrap();

        assert_eq!(signatures[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_table_detail_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
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
        let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let result = registry
            .execute_adhoc(&dsn, "SELECT 1 AS value", true)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
    }

    #[tokio::test]
    async fn sqlite_columns_request_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let detail = registry
            .fetch_table_columns_and_fks(&dsn, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.schema, "main");
        assert_eq!(detail.name, "users");
    }
}

#[async_trait]
impl SqliteDiagnosticsProvider for DbAdapterRegistry {
    async fn fetch_diagnostics_core(
        &self,
        dsn: &str,
        read_only: bool,
    ) -> Result<SqliteDiagnosticsSnapshot, DbOperationError> {
        match Self::db_type_from_dsn(dsn)? {
            DatabaseType::PostgreSQL => Err(DbOperationError::ConnectionFailed(
                "SQLite diagnostics are unavailable for non-SQLite connections".to_string(),
            )),
            DatabaseType::SQLite => self.sqlite.fetch_diagnostics_core(dsn, read_only).await,
        }
    }

    async fn fetch_quick_check(&self, dsn: &str, read_only: bool) -> DiagnosticField {
        match Self::db_type_from_dsn(dsn) {
            Ok(DatabaseType::SQLite) => self.sqlite.fetch_quick_check(dsn, read_only).await,
            Ok(DatabaseType::PostgreSQL) | Err(_) => DiagnosticField::err(
                "SQLite diagnostics are unavailable for non-SQLite connections",
            ),
        }
    }
}

#[cfg(test)]
mod sqlite_diagnostics_registry {
    use super::*;

    #[tokio::test]
    async fn postgres_dsn_is_rejected() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let result = registry
            .fetch_diagnostics_core("postgres://localhost/db", true)
            .await;

        assert!(matches!(result, Err(DbOperationError::ConnectionFailed(_))));
    }
}
