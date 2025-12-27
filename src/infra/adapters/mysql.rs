use async_trait::async_trait;

use crate::app::ports::{DatabaseType, MetadataError, MetadataProvider};
use crate::domain::{DatabaseMetadata, Table};

// TODO: Implement MySQL adapter using mysql CLI
pub struct MySqlAdapter;

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

    fn db_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }
}
