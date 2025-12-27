use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::{DatabaseType, MetadataError, MetadataProvider};
use crate::domain::{
    Column, DatabaseMetadata, FkAction, ForeignKey, Index, IndexType, RlsCommand, RlsInfo,
    RlsPolicy, Schema, Table, TableSummary,
};

pub struct PostgresAdapter {
    timeout_secs: u64,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }

    async fn execute_query(&self, dsn: &str, query: &str) -> Result<String, MetadataError> {
        let result = timeout(
            Duration::from_secs(self.timeout_secs),
            Command::new("psql")
                .arg(dsn)
                .arg("-t") // tuples only
                .arg("-A") // unaligned output
                .arg("-c")
                .arg(query)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| MetadataError::Timeout)?
        .map_err(|e| MetadataError::CommandNotFound(e.to_string()))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(MetadataError::QueryFailed(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    }

    fn tables_query() -> &'static str {
        r#"
        SELECT json_agg(row_to_json(t))
        FROM (
            SELECT
                n.nspname as schema,
                c.relname as name,
                c.reltuples::bigint as row_count_estimate,
                c.relrowsecurity as has_rls
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind = 'r'
              AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY n.nspname, c.relname
        ) t
        "#
    }

    fn schemas_query() -> &'static str {
        r#"
        SELECT json_agg(row_to_json(s))
        FROM (
            SELECT nspname as name
            FROM pg_namespace
            WHERE nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND nspname NOT LIKE 'pg_temp_%'
              AND nspname NOT LIKE 'pg_toast_temp_%'
            ORDER BY nspname
        ) s
        "#
    }

    fn columns_query(schema: &str, table: &str) -> String {
        format!(
            r#"
            SELECT json_agg(row_to_json(c) ORDER BY c.ordinal_position)
            FROM (
                SELECT
                    a.attname as name,
                    pg_catalog.format_type(a.atttypid, a.atttypmod) as data_type,
                    NOT a.attnotnull as nullable,
                    pg_get_expr(d.adbin, d.adrelid) as default,
                    EXISTS (
                        SELECT 1 FROM pg_index i
                        WHERE i.indrelid = cl.oid
                          AND i.indisprimary
                          AND a.attnum = ANY(i.indkey)
                    ) as is_primary_key,
                    EXISTS (
                        SELECT 1 FROM pg_index i
                        WHERE i.indrelid = cl.oid
                          AND i.indisunique
                          AND NOT i.indisprimary
                          AND array_length(i.indkey, 1) = 1
                          AND a.attnum = ANY(i.indkey)
                    ) as is_unique,
                    col_description(cl.oid, a.attnum) as comment,
                    a.attnum as ordinal_position
                FROM pg_class cl
                JOIN pg_namespace n ON n.oid = cl.relnamespace
                JOIN pg_attribute a ON a.attrelid = cl.oid
                LEFT JOIN pg_attrdef d ON d.adrelid = cl.oid AND d.adnum = a.attnum
                WHERE n.nspname = '{}'
                  AND cl.relname = '{}'
                  AND a.attnum > 0
                  AND NOT a.attisdropped
            ) c
            "#,
            schema, table
        )
    }

    fn indexes_query(schema: &str, table: &str) -> String {
        format!(
            r#"
            SELECT json_agg(row_to_json(i))
            FROM (
                SELECT
                    idx.relname as name,
                    array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
                    ix.indisunique as is_unique,
                    ix.indisprimary as is_primary,
                    am.amname as index_type,
                    pg_get_indexdef(idx.oid) as definition
                FROM pg_index ix
                JOIN pg_class idx ON idx.oid = ix.indexrelid
                JOIN pg_class tbl ON tbl.oid = ix.indrelid
                JOIN pg_namespace n ON n.oid = tbl.relnamespace
                JOIN pg_am am ON am.oid = idx.relam
                JOIN pg_attribute a ON a.attrelid = tbl.oid AND a.attnum = ANY(ix.indkey)
                WHERE n.nspname = '{}'
                  AND tbl.relname = '{}'
                GROUP BY idx.relname, ix.indisunique, ix.indisprimary, am.amname, idx.oid
                ORDER BY idx.relname
            ) i
            "#,
            schema, table
        )
    }

    fn foreign_keys_query(schema: &str, table: &str) -> String {
        format!(
            r#"
            SELECT json_agg(row_to_json(fk))
            FROM (
                SELECT
                    con.conname as name,
                    n1.nspname as from_schema,
                    c1.relname as from_table,
                    array_agg(a1.attname ORDER BY array_position(con.conkey, a1.attnum)) as from_columns,
                    n2.nspname as to_schema,
                    c2.relname as to_table,
                    array_agg(a2.attname ORDER BY array_position(con.confkey, a2.attnum)) as to_columns,
                    con.confdeltype as on_delete,
                    con.confupdtype as on_update
                FROM pg_constraint con
                JOIN pg_class c1 ON c1.oid = con.conrelid
                JOIN pg_namespace n1 ON n1.oid = c1.relnamespace
                JOIN pg_class c2 ON c2.oid = con.confrelid
                JOIN pg_namespace n2 ON n2.oid = c2.relnamespace
                JOIN pg_attribute a1 ON a1.attrelid = c1.oid AND a1.attnum = ANY(con.conkey)
                JOIN pg_attribute a2 ON a2.attrelid = c2.oid AND a2.attnum = ANY(con.confkey)
                WHERE con.contype = 'f'
                  AND n1.nspname = '{}'
                  AND c1.relname = '{}'
                GROUP BY con.conname, n1.nspname, c1.relname, n2.nspname, c2.relname, con.confdeltype, con.confupdtype
            ) fk
            "#,
            schema, table
        )
    }

    fn rls_query(schema: &str, table: &str) -> String {
        format!(
            r#"
            SELECT json_build_object(
                'enabled', c.relrowsecurity,
                'force', c.relforcerowsecurity,
                'policies', COALESCE((
                    SELECT json_agg(json_build_object(
                        'name', p.polname,
                        'permissive', p.polpermissive,
                        'roles', (
                            SELECT array_agg(r.rolname)
                            FROM pg_roles r
                            WHERE r.oid = ANY(p.polroles)
                        ),
                        'cmd', p.polcmd,
                        'qual', pg_get_expr(p.polqual, p.polrelid),
                        'with_check', pg_get_expr(p.polwithcheck, p.polrelid)
                    ))
                    FROM pg_policy p
                    WHERE p.polrelid = c.oid
                ), '[]'::json)
            )
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = '{}'
              AND c.relname = '{}'
            "#,
            schema, table
        )
    }

    fn parse_tables(json: &str) -> Result<Vec<TableSummary>, MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(Vec::new());
        }

        #[derive(serde::Deserialize)]
        struct RawTable {
            schema: String,
            name: String,
            row_count_estimate: Option<i64>,
            has_rls: bool,
        }

        let raw: Vec<RawTable> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|t| TableSummary {
                schema: t.schema,
                name: t.name,
                row_count_estimate: t.row_count_estimate,
                has_rls: t.has_rls,
            })
            .collect())
    }

    fn parse_schemas(json: &str) -> Result<Vec<Schema>, MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(Vec::new());
        }

        #[derive(serde::Deserialize)]
        struct RawSchema {
            name: String,
        }

        let raw: Vec<RawSchema> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw.into_iter().map(|s| Schema::new(s.name)).collect())
    }

    fn parse_columns(json: &str) -> Result<Vec<Column>, MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(Vec::new());
        }

        #[derive(serde::Deserialize)]
        struct RawColumn {
            name: String,
            data_type: String,
            nullable: bool,
            default: Option<String>,
            is_primary_key: bool,
            is_unique: bool,
            comment: Option<String>,
            ordinal_position: i32,
        }

        let raw: Vec<RawColumn> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|c| Column {
                name: c.name,
                data_type: c.data_type,
                nullable: c.nullable,
                default: c.default,
                is_primary_key: c.is_primary_key,
                is_unique: c.is_unique,
                comment: c.comment,
                ordinal_position: c.ordinal_position,
            })
            .collect())
    }

    fn parse_indexes(json: &str) -> Result<Vec<Index>, MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(Vec::new());
        }

        #[derive(serde::Deserialize)]
        struct RawIndex {
            name: String,
            columns: Vec<String>,
            is_unique: bool,
            is_primary: bool,
            index_type: String,
            definition: Option<String>,
        }

        let raw: Vec<RawIndex> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|i| Index {
                name: i.name,
                columns: i.columns,
                is_unique: i.is_unique,
                is_primary: i.is_primary,
                index_type: match i.index_type.as_str() {
                    "btree" => IndexType::BTree,
                    "hash" => IndexType::Hash,
                    "gist" => IndexType::Gist,
                    "gin" => IndexType::Gin,
                    "brin" => IndexType::Brin,
                    other => IndexType::Other(other.to_string()),
                },
                definition: i.definition,
            })
            .collect())
    }

    fn parse_foreign_keys(json: &str) -> Result<Vec<ForeignKey>, MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(Vec::new());
        }

        #[derive(serde::Deserialize)]
        struct RawForeignKey {
            name: String,
            from_schema: String,
            from_table: String,
            from_columns: Vec<String>,
            to_schema: String,
            to_table: String,
            to_columns: Vec<String>,
            on_delete: String,
            on_update: String,
        }

        fn parse_fk_action(s: &str) -> FkAction {
            match s {
                "a" => FkAction::NoAction,
                "r" => FkAction::Restrict,
                "c" => FkAction::Cascade,
                "n" => FkAction::SetNull,
                "d" => FkAction::SetDefault,
                _ => FkAction::NoAction,
            }
        }

        let raw: Vec<RawForeignKey> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|fk| ForeignKey {
                name: fk.name,
                from_schema: fk.from_schema,
                from_table: fk.from_table,
                from_columns: fk.from_columns,
                to_schema: fk.to_schema,
                to_table: fk.to_table,
                to_columns: fk.to_columns,
                on_delete: parse_fk_action(&fk.on_delete),
                on_update: parse_fk_action(&fk.on_update),
            })
            .collect())
    }

    fn parse_rls(json: &str) -> Result<Option<RlsInfo>, MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(None);
        }

        #[derive(serde::Deserialize)]
        struct RawRls {
            enabled: bool,
            force: bool,
            policies: Vec<RawPolicy>,
        }

        #[derive(serde::Deserialize)]
        struct RawPolicy {
            name: String,
            permissive: bool,
            roles: Option<Vec<String>>,
            cmd: String,
            qual: Option<String>,
            with_check: Option<String>,
        }

        let raw: RawRls =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        let policies = raw
            .policies
            .into_iter()
            .map(|p| RlsPolicy {
                name: p.name,
                permissive: p.permissive,
                roles: p.roles.unwrap_or_default(),
                cmd: match p.cmd.as_str() {
                    "*" => RlsCommand::All,
                    "r" => RlsCommand::Select,
                    "a" => RlsCommand::Insert,
                    "w" => RlsCommand::Update,
                    "d" => RlsCommand::Delete,
                    _ => RlsCommand::All,
                },
                qual: p.qual,
                with_check: p.with_check,
            })
            .collect();

        Ok(Some(RlsInfo {
            enabled: raw.enabled,
            force: raw.force,
            policies,
        }))
    }

    fn extract_database_name(dsn: &str) -> String {
        dsn.rsplit('/').next().unwrap_or("unknown").to_string()
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for PostgresAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, MetadataError> {
        let schemas_json = self.execute_query(dsn, Self::schemas_query()).await?;
        let tables_json = self.execute_query(dsn, Self::tables_query()).await?;

        let schemas = Self::parse_schemas(&schemas_json)?;
        let tables = Self::parse_tables(&tables_json)?;

        let db_name = Self::extract_database_name(dsn);
        let mut metadata = DatabaseMetadata::new(db_name);
        metadata.schemas = schemas;
        metadata.tables = tables;

        Ok(metadata)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, MetadataError> {
        let columns_json = self
            .execute_query(dsn, &Self::columns_query(schema, table))
            .await?;
        let indexes_json = self
            .execute_query(dsn, &Self::indexes_query(schema, table))
            .await?;
        let fks_json = self
            .execute_query(dsn, &Self::foreign_keys_query(schema, table))
            .await?;
        let rls_json = self
            .execute_query(dsn, &Self::rls_query(schema, table))
            .await?;

        let columns = Self::parse_columns(&columns_json)?;
        let indexes = Self::parse_indexes(&indexes_json)?;
        let foreign_keys = Self::parse_foreign_keys(&fks_json)?;
        let rls = Self::parse_rls(&rls_json)?;

        let primary_key: Option<Vec<String>> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .next()
            .map(|_| columns.iter().filter(|c| c.is_primary_key).map(|c| c.name.clone()).collect());

        Ok(Table {
            schema: schema.to_string(),
            name: table.to_string(),
            columns,
            primary_key,
            foreign_keys,
            indexes,
            rls,
            row_count_estimate: None,
            comment: None,
        })
    }

    fn db_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }
}
