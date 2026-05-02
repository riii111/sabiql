use std::collections::HashMap;

use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{
    Column, ColumnAttributes, DatabaseMetadata, FkAction, ForeignKey, Index, IndexAttributes,
    IndexType, Schema, Table, TableSignature, TableSummary,
};

mod cli;
mod sql;

use cli::SqliteCli;

const MAIN_SCHEMA: &str = "main";

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    cli: SqliteCli,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawTable {
    name: String,
    sql: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawColumn {
    cid: i32,
    name: String,
    #[serde(rename = "type")]
    data_type: String,
    notnull: i64,
    dflt_value: Option<String>,
    pk: i64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawIndex {
    name: String,
    unique: i64,
    origin: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawIndexColumn {
    seqno: i64,
    name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawForeignKey {
    id: i64,
    seq: i64,
    table: String,
    from: String,
    to: Option<String>,
    on_update: String,
    on_delete: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawRowCount {
    count: i64,
}

impl SqliteAdapter {
    pub fn new() -> Self {
        Self {
            cli: SqliteCli::new(),
        }
    }

    fn path_from_dsn(dsn: &str) -> Result<&str, DbOperationError> {
        dsn.strip_prefix("sqlite://")
            .filter(|path| !path.is_empty())
            .ok_or_else(|| DbOperationError::ConnectionFailed(format!("Invalid SQLite DSN: {dsn}")))
    }

    fn database_name(path: &str) -> String {
        std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(path)
            .to_string()
    }

    async fn list_tables(&self, path: &str) -> Result<Vec<RawTable>, DbOperationError> {
        self.cli.execute_json(path, sql::user_tables_query()).await
    }

    async fn row_count(&self, path: &str, table: &str) -> Option<i64> {
        let rows: Result<Vec<RawRowCount>, DbOperationError> = self
            .cli
            .execute_json(path, &sql::row_count_query(table))
            .await;
        rows.ok()
            .and_then(|rows| rows.into_iter().next())
            .map(|row| row.count)
    }

    async fn columns(&self, path: &str, table: &str) -> Result<Vec<RawColumn>, DbOperationError> {
        match self
            .cli
            .execute_json(path, &sql::table_xinfo_query(table))
            .await
        {
            Ok(columns) => Ok(columns),
            Err(_) => {
                self.cli
                    .execute_json(path, &sql::table_info_query(table))
                    .await
            }
        }
    }

    fn extract_primary_key(columns: &[RawColumn]) -> Vec<String> {
        let mut primary_key: Vec<(i64, String)> = columns
            .iter()
            .filter(|column| column.pk > 0)
            .map(|column| (column.pk, column.name.clone()))
            .collect();
        primary_key.sort_by_key(|(pk, _)| *pk);
        primary_key.into_iter().map(|(_, name)| name).collect()
    }

    async fn primary_key_columns(
        &self,
        path: &str,
        table: &str,
    ) -> Result<Vec<String>, DbOperationError> {
        let columns = self.columns(path, table).await?;
        Ok(Self::extract_primary_key(&columns))
    }

    async fn indexes(&self, path: &str, table: &str) -> Result<Vec<Index>, DbOperationError> {
        let raw_indexes: Vec<RawIndex> = self
            .cli
            .execute_json(path, &sql::index_list_query(table))
            .await?;
        let mut indexes = Vec::new();

        for raw in raw_indexes {
            let mut columns: Vec<RawIndexColumn> = self
                .cli
                .execute_json(path, &sql::index_info_query(&raw.name))
                .await?;
            columns.sort_by_key(|col| col.seqno);
            let columns: Vec<String> = columns.into_iter().filter_map(|col| col.name).collect();

            indexes.push(Index {
                name: raw.name,
                columns,
                attributes: IndexAttributes::from_parts(raw.unique != 0, raw.origin == "pk"),
                index_type: IndexType::Other("sqlite".to_string()),
                definition: None,
            });
        }

        indexes.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(indexes)
    }

    async fn foreign_keys(
        &self,
        path: &str,
        table: &str,
    ) -> Result<Vec<ForeignKey>, DbOperationError> {
        let mut raw: Vec<RawForeignKey> = self
            .cli
            .execute_json(path, &sql::foreign_key_list_query(table))
            .await?;
        raw.sort_by_key(|fk| (fk.id, fk.seq));

        let mut grouped = Vec::new();
        let mut current: Option<ForeignKey> = None;
        let mut current_id = None;
        let mut referenced_primary_keys = HashMap::new();

        for fk in raw {
            let to_column = match fk.to {
                Some(to) => to,
                None => {
                    if !referenced_primary_keys.contains_key(&fk.table) {
                        let primary_key = self.primary_key_columns(path, &fk.table).await?;
                        referenced_primary_keys.insert(fk.table.clone(), primary_key);
                    }
                    referenced_primary_keys
                        .get(&fk.table)
                        .and_then(|primary_key| {
                            usize::try_from(fk.seq)
                                .ok()
                                .and_then(|idx| primary_key.get(idx))
                        })
                        .cloned()
                        .ok_or_else(|| {
                            DbOperationError::MetadataParseFailed(format!(
                                "SQLite foreign key references missing primary key column: {}.{}",
                                fk.table, fk.seq
                            ))
                        })?
                }
            };

            if current_id != Some(fk.id) {
                if let Some(fk) = current.take() {
                    grouped.push(fk);
                }
                current_id = Some(fk.id);
                current = Some(ForeignKey {
                    name: format!("fk_{table}_{}", fk.id),
                    from_schema: MAIN_SCHEMA.to_string(),
                    from_table: table.to_string(),
                    from_columns: Vec::new(),
                    to_schema: MAIN_SCHEMA.to_string(),
                    to_table: fk.table.clone(),
                    to_columns: Vec::new(),
                    on_delete: parse_fk_action(&fk.on_delete)?,
                    on_update: parse_fk_action(&fk.on_update)?,
                });
            }

            if let Some(current) = &mut current {
                current.from_columns.push(fk.from);
                current.to_columns.push(to_column);
            }
        }

        if let Some(fk) = current {
            grouped.push(fk);
        }

        Ok(grouped)
    }

    async fn table_detail_with_mode(
        &self,
        path: &str,
        table: &str,
        include_indexes: bool,
    ) -> Result<Table, DbOperationError> {
        let all_indexes = self.indexes(path, table).await?;
        let unique_single_columns = all_indexes
            .iter()
            .filter(|index| index.is_unique() && index.columns.len() == 1)
            .map(|index| index.columns[0].clone())
            .collect::<std::collections::HashSet<_>>();
        let indexes = if include_indexes {
            all_indexes
        } else {
            Vec::new()
        };

        let mut raw_columns = self.columns(path, table).await?;
        if raw_columns.is_empty() {
            return Err(DbOperationError::ObjectMissing(format!(
                "SQLite table not found: {table}"
            )));
        }
        raw_columns.sort_by_key(|column| column.cid);
        let primary_key = Self::extract_primary_key(&raw_columns);
        let columns: Vec<Column> = raw_columns
            .into_iter()
            .map(|column| {
                let is_pk = column.pk > 0;
                Column {
                    name: column.name.clone(),
                    data_type: column.data_type,
                    default: column.dflt_value,
                    attributes: ColumnAttributes::from_parts(
                        column.notnull == 0 && !is_pk,
                        is_pk,
                        unique_single_columns.contains(column.name.as_str()),
                    ),
                    comment: None,
                    ordinal_position: column.cid + 1,
                }
            })
            .collect();
        let primary_key = (!primary_key.is_empty()).then_some(primary_key);

        Ok(Table {
            schema: MAIN_SCHEMA.to_string(),
            name: table.to_string(),
            owner: None,
            columns,
            primary_key,
            foreign_keys: self.foreign_keys(path, table).await?,
            indexes,
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: self.row_count(path, table).await,
            comment: None,
        })
    }

    async fn signature_for_table(
        &self,
        path: &str,
        table: &RawTable,
    ) -> Result<TableSignature, DbOperationError> {
        let detail = self.table_detail_with_mode(path, &table.name, true).await?;
        let mut parts = vec![format!("sql={}", table.sql.clone().unwrap_or_default())];
        parts.extend(detail.columns.iter().map(|column| {
            format!(
                "col={}:{}:{}:{}",
                column.name,
                column.data_type,
                column.is_nullable(),
                column.default.clone().unwrap_or_default()
            )
        }));
        parts.extend(detail.indexes.iter().map(|index| {
            format!(
                "idx={}:{}:{}:{}",
                index.name,
                index.columns.join(","),
                index.is_unique(),
                index.is_primary()
            )
        }));
        parts.extend(detail.foreign_keys.iter().map(|fk| {
            format!(
                "fk={}:{}:{}:{}:{}",
                fk.name,
                fk.from_columns.join(","),
                fk.to_table,
                fk.to_columns.join(","),
                fk.on_delete
            )
        }));

        Ok(TableSignature {
            schema: MAIN_SCHEMA.to_string(),
            name: table.name.clone(),
            signature: parts.join("|"),
        })
    }
}

fn parse_fk_action(action: &str) -> Result<FkAction, DbOperationError> {
    action
        .parse::<FkAction>()
        .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))
}

impl Default for SqliteAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for SqliteAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        let path = Self::path_from_dsn(dsn)?;
        let tables = self.list_tables(path).await?;
        let mut metadata = DatabaseMetadata::new(Self::database_name(path));
        metadata.schemas = vec![Schema::new(MAIN_SCHEMA)];
        for table in &tables {
            metadata.table_summaries.push(TableSummary::new(
                MAIN_SCHEMA.to_string(),
                table.name.clone(),
                self.row_count(path, &table.name).await,
                false,
            ));
        }
        Ok(metadata)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        if schema != MAIN_SCHEMA {
            return Err(DbOperationError::ObjectMissing(format!(
                "SQLite schema not found: {schema}"
            )));
        }
        self.table_detail_with_mode(Self::path_from_dsn(dsn)?, table, true)
            .await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        if schema != MAIN_SCHEMA {
            return Err(DbOperationError::ObjectMissing(format!(
                "SQLite schema not found: {schema}"
            )));
        }
        self.table_detail_with_mode(Self::path_from_dsn(dsn)?, table, false)
            .await
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        let path = Self::path_from_dsn(dsn)?;
        let tables = self.list_tables(path).await?;
        let mut signatures = Vec::new();
        for table in &tables {
            signatures.push(self.signature_for_table(path, table).await?);
        }
        Ok(signatures)
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use crate::app::ports::outbound::MetadataProvider;

    use super::*;

    fn make_db(sql: &str) -> (tempfile::TempDir, String) {
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

    #[tokio::test]
    async fn metadata_lists_user_tables_in_main_schema() {
        let (_dir, dsn) = make_db(
            r#"
            CREATE TABLE users(id INTEGER PRIMARY KEY AUTOINCREMENT);
            "#,
        );
        let adapter = SqliteAdapter::new();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.schemas, vec![Schema::new("main")]);
        assert_eq!(metadata.table_summaries.len(), 1);
        assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
        assert_eq!(metadata.table_summaries[0].row_count_estimate, Some(0));
    }

    #[tokio::test]
    async fn metadata_for_empty_database_returns_no_tables() {
        let (_dir, dsn) = make_db("");
        let adapter = SqliteAdapter::new();

        let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

        assert_eq!(metadata.schemas, vec![Schema::new("main")]);
        assert!(metadata.table_summaries.is_empty());
    }

    #[tokio::test]
    async fn table_detail_loads_columns_indexes_and_foreign_keys() {
        let (_dir, dsn) = make_db(
            r#"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                org_id INTEGER REFERENCES orgs(id) ON DELETE CASCADE
            );
            CREATE INDEX idx_users_org_id ON users(org_id);
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, email, org_id) VALUES (1, 'a@example.com', 1);
            "#,
        );
        let adapter = SqliteAdapter::new();

        let detail = adapter
            .fetch_table_detail(&dsn, "main", "users")
            .await
            .unwrap();

        assert_eq!(detail.primary_key, Some(vec!["id".to_string()]));
        assert_eq!(detail.row_count_estimate, Some(1));
        assert!(detail.columns.iter().any(|column| {
            column.name == "email" && !column.is_nullable() && column.is_unique()
        }));
        assert!(
            detail
                .indexes
                .iter()
                .any(|index| index.name == "idx_users_org_id"
                    && index.columns == vec!["org_id".to_string()])
        );
        let fk = detail
            .foreign_keys
            .iter()
            .find(|fk| fk.to_table == "orgs")
            .unwrap();
        assert_eq!(fk.from_columns, vec!["org_id".to_string()]);
        assert_eq!(fk.to_columns, vec!["id".to_string()]);
        assert_eq!(fk.on_delete, FkAction::Cascade);
        assert!(detail.rls.is_none());
        assert!(detail.triggers.is_empty());
    }

    #[tokio::test]
    async fn table_detail_without_primary_key_sets_primary_key_none() {
        let (_dir, dsn) = make_db("CREATE TABLE logs(message TEXT);");
        let adapter = SqliteAdapter::new();

        let detail = adapter
            .fetch_table_detail(&dsn, "main", "logs")
            .await
            .unwrap();

        assert_eq!(detail.primary_key, None);
        assert_eq!(detail.columns.len(), 1);
    }

    #[tokio::test]
    async fn columns_and_fks_preserves_unique_column_attributes_without_returning_indexes() {
        let (_dir, dsn) = make_db("CREATE TABLE users(email TEXT UNIQUE NOT NULL);");
        let adapter = SqliteAdapter::new();

        let detail = adapter
            .fetch_table_columns_and_fks(&dsn, "main", "users")
            .await
            .unwrap();

        assert!(detail.indexes.is_empty());
        assert!(
            detail
                .columns
                .iter()
                .any(|column| column.name == "email" && column.is_unique())
        );
    }

    #[tokio::test]
    async fn composite_foreign_key_groups_columns_in_sequence_order() {
        let (_dir, dsn) = make_db(
            r#"
            CREATE TABLE parent(a INTEGER, b INTEGER, PRIMARY KEY(a, b));
            CREATE TABLE child(
                x INTEGER,
                y INTEGER,
                FOREIGN KEY(x, y) REFERENCES parent(a, b)
            );
            "#,
        );
        let adapter = SqliteAdapter::new();

        let detail = adapter
            .fetch_table_detail(&dsn, "main", "child")
            .await
            .unwrap();

        assert_eq!(detail.foreign_keys.len(), 1);
        assert_eq!(
            detail.foreign_keys[0].from_columns,
            vec!["x".to_string(), "y".to_string()]
        );
        assert_eq!(
            detail.foreign_keys[0].to_columns,
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[tokio::test]
    async fn foreign_key_without_target_columns_resolves_parent_primary_key() {
        let (_dir, dsn) = make_db(
            r#"
            CREATE TABLE parent(a INTEGER, b INTEGER, PRIMARY KEY(a, b));
            CREATE TABLE child(
                x INTEGER,
                y INTEGER,
                FOREIGN KEY(x, y) REFERENCES parent
            );
            "#,
        );
        let adapter = SqliteAdapter::new();

        let detail = adapter
            .fetch_table_detail(&dsn, "main", "child")
            .await
            .unwrap();

        assert_eq!(
            detail.foreign_keys[0].to_columns,
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[tokio::test]
    async fn invalid_dsn_returns_connection_error() {
        let adapter = SqliteAdapter::new();

        let postgres_result = adapter.fetch_metadata("postgres://localhost").await;
        let empty_result = adapter.fetch_metadata("sqlite://").await;

        assert!(matches!(
            postgres_result,
            Err(DbOperationError::ConnectionFailed(_))
        ));
        assert!(matches!(
            empty_result,
            Err(DbOperationError::ConnectionFailed(_))
        ));
    }

    #[tokio::test]
    async fn missing_database_file_returns_error_without_creating_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let dsn = format!("sqlite://{}", path.display());
        let adapter = SqliteAdapter::new();

        let result = adapter.fetch_metadata(&dsn).await;

        assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn non_main_schema_returns_object_missing() {
        let (_dir, dsn) = make_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let result = adapter.fetch_table_detail(&dsn, "other", "users").await;

        assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
    }

    #[tokio::test]
    async fn missing_table_returns_object_missing() {
        let (_dir, dsn) = make_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let result = adapter.fetch_table_detail(&dsn, "main", "missing").await;

        assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
    }

    #[tokio::test]
    async fn table_signatures_change_with_table_shape() {
        let (_dir, dsn) = make_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();

        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0].qualified_name(), "main.users");
        assert!(signatures[0].signature.contains("CREATE TABLE users"));
        assert!(signatures[0].signature.contains("col=id:INTEGER"));
    }
}
