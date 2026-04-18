use async_trait::async_trait;

use crate::app::model::shared::db_capabilities::DbCapabilities;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::ports::{
    DbOperationError, DdlGenerator, DsnBuilder, MetadataProvider, QueryExecutor, SqlDialect,
};
use crate::domain::connection::ConnectionProfile;
use crate::domain::{DatabaseMetadata, QueryResult, Table, TableSignature, WriteExecutionResult};

pub struct MySqlAdapter;

impl MySqlAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn capabilities(&self) -> DbCapabilities {
        DbCapabilities::new(
            false,
            vec![
                InspectorTab::Info,
                InspectorTab::Columns,
                InspectorTab::Indexes,
                InspectorTab::ForeignKeys,
                InspectorTab::Ddl,
            ],
        )
    }
}

impl Default for MySqlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for MySqlAdapter {
    async fn fetch_metadata(&self, _dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn fetch_table_detail(
        &self,
        _dsn: &str,
        _schema: &str,
        _table: &str,
    ) -> Result<Table, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn fetch_table_columns_and_fks(
        &self,
        _dsn: &str,
        _schema: &str,
        _table: &str,
    ) -> Result<Table, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn fetch_table_signatures(
        &self,
        _dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }
}

#[async_trait]
impl QueryExecutor for MySqlAdapter {
    async fn execute_preview(
        &self,
        _dsn: &str,
        _schema: &str,
        _table: &str,
        _limit: usize,
        _offset: usize,
        _read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn execute_adhoc(
        &self,
        _dsn: &str,
        _query: &str,
        _read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn execute_write(
        &self,
        _dsn: &str,
        _query: &str,
        _read_only: bool,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn count_query_rows(
        &self,
        _dsn: &str,
        _query: &str,
        _read_only: bool,
    ) -> Result<usize, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn export_to_csv(
        &self,
        _dsn: &str,
        _query: &str,
        _path: &std::path::Path,
        _read_only: bool,
    ) -> Result<usize, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }
}

impl DdlGenerator for MySqlAdapter {
    fn generate_ddl(&self, _table: &Table) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }
}

impl SqlDialect for MySqlAdapter {
    fn build_explain_sql(&self, query: &str) -> String {
        // Fallback that keeps behavior total even when capability checks are missed.
        format!("EXPLAIN {query}")
    }

    fn build_explain_analyze_sql(&self, query: &str) -> String {
        // Fallback keeps behavior total for defensive coverage.
        format!("EXPLAIN ANALYZE {query}")
    }

    fn build_update_sql(
        &self,
        _schema: &str,
        _table: &str,
        _column: &str,
        _new_value: &str,
        _pk_pairs: &[(String, String)],
    ) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }

    fn build_bulk_delete_sql(
        &self,
        _schema: &str,
        _table: &str,
        _pk_pairs_per_row: &[Vec<(String, String)>],
    ) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }
}

impl DsnBuilder for MySqlAdapter {
    fn build_dsn(&self, _profile: &ConnectionProfile) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }
}
