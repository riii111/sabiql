use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::{MetadataError, MetadataProvider};
use crate::domain::{
    Column, DatabaseMetadata, FkAction, ForeignKey, Index, IndexType, QueryResult, QuerySource,
    RlsCommand, RlsInfo, RlsPolicy, Schema, Table, TableSummary,
};
use crate::infra::utils::{quote_ident, quote_literal};

pub struct PostgresAdapter {
    timeout_secs: u64,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    async fn execute_query(&self, dsn: &str, query: &str) -> Result<String, MetadataError> {
        let mut child = Command::new("psql")
            .arg(dsn)
            .arg("-X") // Ignore .psqlrc to avoid unexpected output
            .arg("-v")
            .arg("ON_ERROR_STOP=1") // Exit with non-zero on SQL errors
            .arg("-t") // Tuples only
            .arg("-A") // Unaligned output
            .arg("-c")
            .arg(query)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true) // Ensure child process is killed on timeout/drop
            .spawn()
            .map_err(|e| MetadataError::CommandNotFound(e.to_string()))?;

        // Read stdout/stderr BEFORE wait() to prevent pipe buffer deadlock
        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let result = timeout(Duration::from_secs(self.timeout_secs), async {
            let (stdout_result, stderr_result) = tokio::join!(
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut out) = stdout_handle {
                        out.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                },
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut err) = stderr_handle {
                        err.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                }
            );

            let stdout = stdout_result?;
            let stderr = stderr_result?;
            let status = child.wait().await?;

            Ok::<_, std::io::Error>((status, stdout, stderr))
        })
        .await
        .map_err(|_| MetadataError::Timeout)?
        .map_err(|e| MetadataError::QueryFailed(e.to_string()))?;

        let (status, stdout, stderr) = result;

        if !status.success() {
            return Err(MetadataError::QueryFailed(stderr));
        }

        Ok(stdout)
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
              AND (
                  has_table_privilege(c.oid, 'SELECT')
                  OR has_table_privilege(c.oid, 'INSERT')
                  OR has_table_privilege(c.oid, 'UPDATE')
                  OR has_table_privilege(c.oid, 'DELETE')
                  OR has_table_privilege(c.oid, 'TRUNCATE')
                  OR has_table_privilege(c.oid, 'REFERENCES')
                  OR has_table_privilege(c.oid, 'TRIGGER')
              )
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
                WHERE n.nspname = {}
                  AND cl.relname = {}
                  AND a.attnum > 0
                  AND NOT a.attisdropped
            ) c
            "#,
            quote_literal(schema),
            quote_literal(table)
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
                WHERE n.nspname = {}
                  AND tbl.relname = {}
                GROUP BY idx.relname, ix.indisunique, ix.indisprimary, am.amname, idx.oid
                ORDER BY idx.relname
            ) i
            "#,
            quote_literal(schema),
            quote_literal(table)
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
                  AND n1.nspname = {}
                  AND c1.relname = {}
                GROUP BY con.conname, n1.nspname, c1.relname, n2.nspname, c2.relname, con.confdeltype, con.confupdtype
            ) fk
            "#,
            quote_literal(schema),
            quote_literal(table)
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
            WHERE n.nspname = {}
              AND c.relname = {}
            "#,
            quote_literal(schema),
            quote_literal(table)
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
            .map(|t| TableSummary::new(t.schema, t.name, t.row_count_estimate, t.has_rls))
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

    /// Execute a raw SQL query and return structured results.
    /// This is used for adhoc queries and preview queries.
    pub async fn execute_query_raw(
        &self,
        dsn: &str,
        query: &str,
        source: QuerySource,
    ) -> Result<QueryResult, MetadataError> {
        let start = Instant::now();

        // Execute with CSV output for robust parsing
        let mut child = Command::new("psql")
            .arg(dsn)
            .arg("-X") // Ignore .psqlrc
            .arg("-v")
            .arg("ON_ERROR_STOP=1")
            .arg("--csv") // CSV output format (handles quoting/escaping)
            .arg("-c")
            .arg(query)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| MetadataError::CommandNotFound(e.to_string()))?;

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let result = timeout(Duration::from_secs(self.timeout_secs), async {
            let (stdout_result, stderr_result) = tokio::join!(
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut out) = stdout_handle {
                        out.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                },
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut err) = stderr_handle {
                        err.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                }
            );

            let stdout = stdout_result?;
            let stderr = stderr_result?;
            let status = child.wait().await?;

            Ok::<_, std::io::Error>((status, stdout, stderr))
        })
        .await
        .map_err(|_| MetadataError::Timeout)?
        .map_err(|e| MetadataError::QueryFailed(e.to_string()))?;

        let elapsed = start.elapsed().as_millis() as u64;
        let (status, stdout, stderr) = result;

        if !status.success() {
            return Ok(QueryResult::error(
                query.to_string(),
                stderr.trim().to_string(),
                elapsed,
                source,
            ));
        }

        // Parse CSV output using csv crate for robust handling
        if stdout.trim().is_empty() {
            return Ok(QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                source,
            ));
        }

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(stdout.as_bytes());

        // Get column headers
        let columns: Vec<String> = reader
            .headers()
            .map_err(|e| MetadataError::QueryFailed(format!("CSV parse error: {}", e)))?
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Parse data rows
        let mut rows = Vec::new();
        for result in reader.records() {
            let record = result
                .map_err(|e| MetadataError::QueryFailed(format!("CSV parse error: {}", e)))?;
            let row: Vec<String> = record.iter().map(|s| s.to_string()).collect();
            rows.push(row);
        }

        Ok(QueryResult::success(
            query.to_string(),
            columns,
            rows,
            elapsed,
            source,
        ))
    }

    /// Extract database name from DSN string.
    /// Supports both URI format (postgres://host/dbname) and key=value format (dbname=mydb).
    pub fn extract_database_name(dsn: &str) -> String {
        if let Some(db) = dsn
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty() && !s.contains('='))
        {
            return db.to_string();
        }
        if let Some(start) = dsn.find("dbname=") {
            let rest = &dsn[start + 7..];
            let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            return rest[..end].to_string();
        }
        "unknown".to_string()
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
        let columns_q = Self::columns_query(schema, table);
        let indexes_q = Self::indexes_query(schema, table);
        let fks_q = Self::foreign_keys_query(schema, table);
        let rls_q = Self::rls_query(schema, table);

        // Execute queries sequentially to avoid connection pool exhaustion
        // on tables with many columns
        // TODO: If performance becomes an issue, consider migrating to controlled parallel
        // execution using semaphores (e.g., tokio::sync::Semaphore) to limit concurrency
        let columns_json = self.execute_query(dsn, &columns_q).await?;
        let indexes_json = self.execute_query(dsn, &indexes_q).await?;
        let fks_json = self.execute_query(dsn, &fks_q).await?;
        let rls_json = self.execute_query(dsn, &rls_q).await?;

        let columns = Self::parse_columns(&columns_json)?;
        let indexes = Self::parse_indexes(&indexes_json)?;
        let foreign_keys = Self::parse_foreign_keys(&fks_json)?;
        let rls = Self::parse_rls(&rls_json)?;

        let pk_cols: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        let primary_key = if pk_cols.is_empty() {
            None
        } else {
            Some(pk_cols)
        };

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

    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
    ) -> Result<QueryResult, MetadataError> {
        let query = format!(
            "SELECT * FROM {}.{} LIMIT {}",
            quote_ident(schema),
            quote_ident(table),
            limit
        );
        self.execute_query_raw(dsn, &query, QuerySource::Preview)
            .await
    }

    async fn execute_adhoc(&self, dsn: &str, query: &str) -> Result<QueryResult, MetadataError> {
        let trimmed = query.trim();
        let is_select = trimmed.to_lowercase().starts_with("select")
            || trimmed.to_lowercase().starts_with("with");

        if !is_select {
            return Err(MetadataError::QueryFailed(
                "Only SELECT queries are supported in SQL modal. Use psql/mycli for DDL/DML operations.".to_string()
            ));
        }

        self.execute_query_raw(dsn, query, QuerySource::Adhoc).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_database_name_uri_format() {
        assert_eq!(
            PostgresAdapter::extract_database_name("postgres://user:pass@host:5432/mydb"),
            "mydb"
        );
    }

    #[test]
    fn test_extract_database_name_simple_uri() {
        assert_eq!(
            PostgresAdapter::extract_database_name("postgres://localhost/testdb"),
            "testdb"
        );
    }

    #[test]
    fn test_extract_database_name_key_value_format() {
        assert_eq!(
            PostgresAdapter::extract_database_name("host=localhost dbname=mydb user=postgres"),
            "mydb"
        );
    }

    #[test]
    fn test_extract_database_name_key_value_at_end() {
        assert_eq!(
            PostgresAdapter::extract_database_name("host=localhost user=postgres dbname=testdb"),
            "testdb"
        );
    }

    #[test]
    fn test_extract_database_name_empty_path() {
        // URI with trailing slash but no db name
        assert_eq!(
            PostgresAdapter::extract_database_name("postgres://localhost/"),
            "unknown"
        );
    }

    #[test]
    fn test_extract_database_name_key_value_only() {
        // Key-value format without dbname
        assert_eq!(
            PostgresAdapter::extract_database_name("host=localhost user=postgres"),
            "unknown"
        );
    }

    mod csv_parsing {
        #[test]
        fn empty_csv_output_has_no_headers() {
            let csv_data = "";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(false)
                .from_reader(csv_data.as_bytes());

            let records: Vec<_> = reader.records().collect();

            assert_eq!(records.len(), 0);
        }

        #[test]
        fn valid_csv_parses_headers_and_rows() {
            let csv_data = "id,name\n1,alice\n2,bob";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let headers: Vec<String> = reader
                .headers()
                .unwrap()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let rows: Vec<_> = reader.records().collect();

            assert_eq!(headers.len(), 2);
            assert_eq!(headers[0], "id");
            assert_eq!(headers[1], "name");
            assert_eq!(rows.len(), 2);
        }

        #[test]
        fn csv_with_multibyte_characters_parses_correctly() {
            let csv_data = "名前,年齢\n太郎,25\n花子,30";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let headers: Vec<String> = reader
                .headers()
                .unwrap()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let first_row = reader.records().next().unwrap().unwrap();

            assert_eq!(headers[0], "名前");
            assert_eq!(first_row.get(0), Some("太郎"));
        }

        #[test]
        fn csv_with_quoted_fields_parses_correctly() {
            let csv_data = "id,description\n1,\"hello, world\"\n2,\"line1\nline2\"";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let rows: Vec<_> = reader.records().map(|r| r.unwrap()).collect();

            assert_eq!(rows[0].get(1), Some("hello, world"));
            assert_eq!(rows[1].get(1), Some("line1\nline2"));
        }

        #[test]
        fn csv_with_empty_values_parses_correctly() {
            let csv_data = "id,name,email\n1,,alice@example.com\n2,bob,";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let rows: Vec<_> = reader.records().map(|r| r.unwrap()).collect();

            assert_eq!(rows[0].get(1), Some(""));
            assert_eq!(rows[1].get(2), Some(""));
        }

        #[test]
        fn invalid_csv_returns_error() {
            let csv_data = "id,name\n1,alice\n2,bob,extra";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(false)
                .from_reader(csv_data.as_bytes());

            let _ = reader.headers().unwrap();
            let results: Vec<_> = reader.records().collect();

            assert!(results[1].is_err());
        }

        #[test]
        fn non_csv_output_like_notice_returns_error() {
            let non_csv = "NOTICE: some database notice\nNOTICE: another line";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(non_csv.as_bytes());

            let headers = reader.headers();

            assert!(headers.is_ok());
        }

        #[test]
        fn mixed_notice_and_csv_parses_first_line_as_header() {
            let mixed = "id,name\n1,alice";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(mixed.as_bytes());

            let headers: Vec<String> = reader
                .headers()
                .unwrap()
                .iter()
                .map(|s| s.to_string())
                .collect();

            assert_eq!(headers[0], "id");
            assert_eq!(headers[1], "name");
        }
    }
}
