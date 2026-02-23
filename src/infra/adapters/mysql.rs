use async_trait::async_trait;

use crate::app::ports::{
    DdlGenerator, DsnBuilder, MetadataError, MetadataProvider, QueryExecutor, SqlDialect,
};
use crate::domain::connection::ConnectionProfile;
use crate::domain::{DatabaseMetadata, QueryResult, Table, WriteExecutionResult};

#[allow(dead_code)]
pub struct MySqlAdapter;

#[allow(dead_code)]
impl MySqlAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MySqlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for MySqlAdapter {
    async fn fetch_metadata(&self, _dsn: &str) -> Result<DatabaseMetadata, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn fetch_table_detail(
        &self,
        _dsn: &str,
        _schema: &str,
        _table: &str,
    ) -> Result<Table, MetadataError> {
        Err(MetadataError::ConnectionFailed(
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
    ) -> Result<QueryResult, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn execute_adhoc(&self, _dsn: &str, _query: &str) -> Result<QueryResult, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }

    async fn execute_write(
        &self,
        _dsn: &str,
        _query: &str,
    ) -> Result<WriteExecutionResult, MetadataError> {
        Err(MetadataError::ConnectionFailed(
            "MySQL adapter not yet implemented".to_string(),
        ))
    }
}

impl DdlGenerator for MySqlAdapter {
    fn generate_ddl(&self, _table: &Table) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }

    fn ddl_line_count(&self, _table: &Table) -> usize {
        unimplemented!("MySQL adapter not yet implemented")
    }
}

impl SqlDialect for MySqlAdapter {
    fn quote_ident(&self, _name: &str) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }

    fn quote_literal(&self, _value: &str) -> String {
        unimplemented!("MySQL adapter not yet implemented")
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

    fn build_masked_dsn(&self, _profile: &ConnectionProfile) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }
}
