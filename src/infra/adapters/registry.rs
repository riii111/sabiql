use std::path::Path;
use std::sync::Arc;

use app::ports::outbound::{
    DbOperationError, DdlGenerator, DsnBuilder, MetadataProvider, QueryExecutor, SqlDialect,
};
use async_trait::async_trait;
use domain::connection::{ConnectionProfile, DatabaseType};
use domain::{DatabaseMetadata, QueryResult, Table, TableSignature, WriteExecutionResult};

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

    fn sqlite_query_not_implemented() -> DbOperationError {
        DbOperationError::QueryFailed("SQLite query execution is not implemented yet".to_string())
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
            DatabaseType::SQLite => Err(Self::sqlite_query_not_implemented()),
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
            DatabaseType::SQLite => Err(Self::sqlite_query_not_implemented()),
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
            DatabaseType::SQLite => Err(Self::sqlite_query_not_implemented()),
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
            DatabaseType::SQLite => Err(Self::sqlite_query_not_implemented()),
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
            DatabaseType::SQLite => Err(Self::sqlite_query_not_implemented()),
        }
    }
}

// SQLite query and DDL support is intentionally out of scope for SAB-235.
impl DdlGenerator for DbAdapterRegistry {
    fn generate_ddl(&self, table: &Table) -> String {
        self.postgres.generate_ddl(table)
    }
}

impl SqlDialect for DbAdapterRegistry {
    fn build_explain_sql(&self, query: &str) -> Option<String> {
        self.postgres.build_explain_sql(query)
    }

    fn build_explain_analyze_sql(&self, query: &str) -> Option<String> {
        self.postgres.build_explain_analyze_sql(query)
    }

    fn build_update_sql(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &str,
        pk_pairs: &[(String, String)],
    ) -> String {
        self.postgres
            .build_update_sql(schema, table, column, new_value, pk_pairs)
    }

    fn build_bulk_delete_sql(
        &self,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, String)>],
    ) -> String {
        self.postgres
            .build_bulk_delete_sql(schema, table, pk_pairs_per_row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::connection::SslMode;
    use std::process::Command;

    fn make_sqlite_dsn(sql: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let status = Command::new("sqlite3")
            .arg(&path)
            .arg(sql)
            .status()
            .unwrap();
        assert!(status.success());
        (dir, format!("sqlite://{}", path.display()))
    }

    #[test]
    fn builds_postgres_dsn_from_postgres_profile() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));
        let profile = ConnectionProfile::new(
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

    #[tokio::test]
    async fn unknown_dsn_scheme_is_rejected() {
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let result = registry.fetch_metadata("mysql://localhost/db").await;

        assert!(matches!(result, Err(DbOperationError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn sqlite_metadata_is_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) = make_sqlite_dsn("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let metadata = registry.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
    }

    #[tokio::test]
    async fn sqlite_table_signatures_are_dispatched_to_sqlite_adapter() {
        let (_dir, dsn) = make_sqlite_dsn("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let registry = DbAdapterRegistry::new(Arc::new(PostgresAdapter::new()));

        let signatures = registry.fetch_table_signatures(&dsn).await.unwrap();

        assert_eq!(signatures[0].qualified_name(), "main.users");
    }
}
