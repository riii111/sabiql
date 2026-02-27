use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::{DdlGenerator, MetadataError, MetadataProvider, QueryExecutor, SqlDialect};
use crate::domain::{DatabaseMetadata, QueryResult, QuerySource, Table, WriteExecutionResult};
use crate::infra::utils::{quote_ident, quote_literal};

mod dsn;
mod psql;
mod select_guard;

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

    fn preview_pk_columns_query(schema: &str, table: &str) -> String {
        format!(
            r#"
            SELECT COALESCE(json_agg(a.attname ORDER BY array_position(i.indkey, a.attnum)), '[]'::json)
            FROM pg_index i
            JOIN pg_class c ON c.oid = i.indrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
            WHERE i.indisprimary
              AND n.nspname = {}
              AND c.relname = {}
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
    }

    async fn fetch_preview_order_columns(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Vec<String>, MetadataError> {
        let query = Self::preview_pk_columns_query(schema, table);
        let raw = self.execute_query(dsn, &query).await?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(vec![]);
        }

        serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))
    }

    fn build_preview_query(
        schema: &str,
        table: &str,
        order_columns: &[String],
        limit: usize,
        offset: usize,
    ) -> String {
        let order_clause = if order_columns.is_empty() {
            String::new()
        } else {
            let cols = order_columns
                .iter()
                .map(|col| quote_ident(col))
                .collect::<Vec<_>>()
                .join(", ");
            format!(" ORDER BY {}", cols)
        };

        format!(
            "SELECT * FROM {}.{}{} LIMIT {} OFFSET {}",
            quote_ident(schema),
            quote_ident(table),
            order_clause,
            limit,
            offset
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

    fn triggers_query(schema: &str, table: &str) -> String {
        format!(
            r#"
            SELECT json_agg(row_to_json(t) ORDER BY t.name)
            FROM (
                SELECT
                    tg.tgname AS name,
                    CASE
                        WHEN (tg.tgtype & 2) != 0 THEN 'BEFORE'
                        WHEN (tg.tgtype & 2) = 0 AND (tg.tgtype & 64) != 0 THEN 'INSTEAD OF'
                        ELSE 'AFTER'
                    END AS timing,
                    array_remove(ARRAY[
                        CASE WHEN (tg.tgtype & 4) != 0 THEN 'INSERT' END,
                        CASE WHEN (tg.tgtype & 8) != 0 THEN 'DELETE' END,
                        CASE WHEN (tg.tgtype & 16) != 0 THEN 'UPDATE' END,
                        CASE WHEN (tg.tgtype & 32) != 0 THEN 'TRUNCATE' END
                    ], NULL) AS events,
                    p.proname AS function_name,
                    p.prosecdef AS security_definer
                FROM pg_trigger tg
                JOIN pg_class c ON c.oid = tg.tgrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_proc p ON p.oid = tg.tgfoid
                WHERE NOT tg.tgisinternal
                  AND n.nspname = {}
                  AND c.relname = {}
            ) t
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
    }

    fn table_info_query(schema: &str, table: &str) -> String {
        format!(
            r#"
            SELECT row_to_json(t)
            FROM (
                SELECT
                    pg_get_userbyid(c.relowner) AS owner,
                    obj_description(c.oid) AS comment,
                    c.reltuples::bigint AS row_count_estimate
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = {}
                  AND c.relname = {}
            ) t
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
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

    pub async fn execute_write_raw(
        &self,
        dsn: &str,
        query: &str,
    ) -> Result<WriteExecutionResult, MetadataError> {
        let start = Instant::now();

        let mut child = Command::new("psql")
            .arg(dsn)
            .arg("-X")
            .arg("-v")
            .arg("ON_ERROR_STOP=1")
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
            return Err(MetadataError::QueryFailed(stderr.trim().to_string()));
        }

        let affected_rows = Self::parse_affected_rows(&stdout).ok_or_else(|| {
            MetadataError::QueryFailed("Failed to parse affected row count".to_string())
        })?;

        Ok(WriteExecutionResult {
            affected_rows,
            execution_time_ms: elapsed,
        })
    }

    fn parse_affected_rows(stdout: &str) -> Option<usize> {
        stdout.lines().rev().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() != 2 {
                return None;
            }
            match parts[0] {
                "UPDATE" | "DELETE" => parts[1].parse::<usize>().ok(),
                _ => None,
            }
        })
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
        let triggers_q = Self::triggers_query(schema, table);
        let table_info_q = Self::table_info_query(schema, table);

        // Execute queries sequentially to avoid connection pool exhaustion
        // on tables with many columns
        // TODO: If performance becomes an issue, consider migrating to controlled parallel
        // execution using semaphores (e.g., tokio::sync::Semaphore) to limit concurrency
        let columns_json = self.execute_query(dsn, &columns_q).await?;
        let indexes_json = self.execute_query(dsn, &indexes_q).await?;
        let fks_json = self.execute_query(dsn, &fks_q).await?;
        let rls_json = self.execute_query(dsn, &rls_q).await?;
        let triggers_json = self.execute_query(dsn, &triggers_q).await?;
        let table_info_json = self.execute_query(dsn, &table_info_q).await?;

        let columns = Self::parse_columns(&columns_json)?;
        let indexes = Self::parse_indexes(&indexes_json)?;
        let foreign_keys = Self::parse_foreign_keys(&fks_json)?;
        let rls = Self::parse_rls(&rls_json)?;
        let triggers = Self::parse_triggers(&triggers_json)?;
        let (owner, comment, row_count_estimate) = Self::parse_table_info(&table_info_json)?;

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
            owner,
            columns,
            primary_key,
            foreign_keys,
            indexes,
            rls,
            triggers,
            row_count_estimate,
            comment,
        })
    }
}

#[async_trait]
impl QueryExecutor for PostgresAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, MetadataError> {
        // Editing a cell re-fetches the same page; stable ordering prevents the
        // edited row from shifting position after the refresh.
        let order_columns = self
            .fetch_preview_order_columns(dsn, schema, table)
            .await
            .unwrap_or_default();
        let query = Self::build_preview_query(schema, table, &order_columns, limit, offset);
        self.execute_query_raw(dsn, &query, QuerySource::Preview)
            .await
    }

    async fn execute_adhoc(&self, dsn: &str, query: &str) -> Result<QueryResult, MetadataError> {
        if !select_guard::is_select_query(query) {
            return Err(MetadataError::QueryFailed(
                "Only SELECT queries are supported in SQL modal. Use psql/mycli for DDL/DML operations.".to_string()
            ));
        }

        self.execute_query_raw(dsn, query, QuerySource::Adhoc).await
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
    ) -> Result<WriteExecutionResult, MetadataError> {
        self.execute_write_raw(dsn, query).await
    }
}

fn pg_quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn pg_quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn pg_sql_value_expr(value: &str) -> String {
    if value == "NULL" {
        "NULL".to_string()
    } else {
        pg_quote_literal(value)
    }
}

impl DdlGenerator for PostgresAdapter {
    fn generate_ddl(&self, table: &Table) -> String {
        let mut ddl = format!(
            "CREATE TABLE {}.{} (\n",
            pg_quote_ident(&table.schema),
            pg_quote_ident(&table.name)
        );

        for (i, col) in table.columns.iter().enumerate() {
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map(|d| format!(" DEFAULT {}", d))
                .unwrap_or_default();

            ddl.push_str(&format!(
                "  {} {}{}{}",
                pg_quote_ident(&col.name),
                col.data_type,
                nullable,
                default
            ));

            if i < table.columns.len() - 1 {
                ddl.push(',');
            }
            ddl.push('\n');
        }

        if let Some(pk) = &table.primary_key {
            let quoted_cols: Vec<String> = pk.iter().map(|c| pg_quote_ident(c)).collect();
            ddl.push_str(&format!("  PRIMARY KEY ({})\n", quoted_cols.join(", ")));
        }

        ddl.push_str(");");

        let qualified = format!(
            "{}.{}",
            pg_quote_ident(&table.schema),
            pg_quote_ident(&table.name)
        );

        if let Some(comment) = &table.comment {
            ddl.push_str(&format!(
                "\n\nCOMMENT ON TABLE {} IS {};",
                qualified,
                pg_quote_literal(comment)
            ));
        }

        for col in &table.columns {
            if let Some(comment) = &col.comment {
                ddl.push_str(&format!(
                    "\n\nCOMMENT ON COLUMN {}.{} IS {};",
                    qualified,
                    pg_quote_ident(&col.name),
                    pg_quote_literal(comment)
                ));
            }
        }

        ddl
    }
}

impl SqlDialect for PostgresAdapter {
    fn build_update_sql(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &str,
        pk_pairs: &[(String, String)],
    ) -> String {
        let where_clause = pk_pairs
            .iter()
            .map(|(col, val)| format!("{} = {}", pg_quote_ident(col), pg_quote_literal(val)))
            .collect::<Vec<_>>()
            .join(" AND ");

        format!(
            "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
            pg_quote_ident(schema),
            pg_quote_ident(table),
            pg_quote_ident(column),
            pg_sql_value_expr(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, String)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let pk_count = pk_pairs_per_row[0].len();

        let where_clause = if pk_count == 1 {
            let col = pg_quote_ident(&pk_pairs_per_row[0][0].0);
            let values = pk_pairs_per_row
                .iter()
                .map(|pairs| pg_sql_value_expr(&pairs[0].1))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} IN ({})", col, values)
        } else {
            let cols = pk_pairs_per_row[0]
                .iter()
                .map(|(col, _)| pg_quote_ident(col))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = pk_pairs_per_row
                .iter()
                .map(|pairs| {
                    let vals = pairs
                        .iter()
                        .map(|(_, val)| pg_sql_value_expr(val))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({})", vals)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) IN ({})", cols, rows)
        };

        format!(
            "DELETE FROM {}.{}\nWHERE {};",
            pg_quote_ident(schema),
            pg_quote_ident(table),
            where_clause
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod preview_query {
        use super::*;

        #[test]
        fn with_primary_key_columns_returns_ordered_preview_query() {
            let sql = PostgresAdapter::build_preview_query(
                "public",
                "users",
                &["id".to_string(), "tenant_id".to_string()],
                100,
                200,
            );

            assert_eq!(
                sql,
                "SELECT * FROM \"public\".\"users\" ORDER BY \"id\", \"tenant_id\" LIMIT 100 OFFSET 200"
            );
        }

        #[test]
        fn without_primary_key_columns_returns_unordered_preview_query() {
            let sql = PostgresAdapter::build_preview_query("public", "users", &[], 100, 0);

            assert_eq!(sql, "SELECT * FROM \"public\".\"users\" LIMIT 100 OFFSET 0");
        }

        #[test]
        fn primary_key_query_returns_json_aggregate_sql() {
            let sql = PostgresAdapter::preview_pk_columns_query("public", "users");

            assert!(
                sql.contains("json_agg(a.attname ORDER BY array_position(i.indkey, a.attnum))")
            );
            assert!(sql.contains("n.nspname = 'public'"));
            assert!(sql.contains("c.relname = 'users'"));
        }
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

    mod write_command_tag {
        use super::*;

        #[test]
        fn parse_affected_rows_for_update() {
            let out = "UPDATE 1\n";
            assert_eq!(PostgresAdapter::parse_affected_rows(out), Some(1));
        }

        #[test]
        fn parse_affected_rows_for_delete() {
            let out = "DELETE 3\n";
            assert_eq!(PostgresAdapter::parse_affected_rows(out), Some(3));
        }

        #[test]
        fn parse_affected_rows_returns_none_for_unknown_output() {
            let out = "SELECT 1\n";
            assert_eq!(PostgresAdapter::parse_affected_rows(out), None);
        }
    }

    mod ddl_generation {
        use super::*;
        use crate::app::ports::DdlGenerator;
        use crate::domain::Column;

        fn make_column(name: &str, data_type: &str, nullable: bool) -> Column {
            Column {
                name: name.to_string(),
                data_type: data_type.to_string(),
                nullable,
                is_primary_key: false,
                default: None,
                is_unique: false,
                comment: None,
                ordinal_position: 0,
            }
        }

        fn make_table(columns: Vec<Column>, primary_key: Option<Vec<String>>) -> Table {
            Table {
                schema: "public".to_string(),
                name: "test_table".to_string(),
                owner: None,
                columns,
                primary_key,
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        #[test]
        fn table_with_pk_returns_valid_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(
                vec![
                    make_column("id", "integer", false),
                    make_column("name", "text", true),
                ],
                Some(vec!["id".to_string()]),
            );

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("CREATE TABLE \"public\".\"test_table\""));
            assert!(ddl.contains("\"id\" integer NOT NULL"));
            assert!(ddl.contains("\"name\" text"));
            assert!(ddl.contains("PRIMARY KEY (\"id\")"));
        }

        #[test]
        fn table_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("User accounts".to_string());

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("COMMENT ON TABLE \"public\".\"test_table\" IS 'User accounts';"));
        }

        #[test]
        fn column_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut col = make_column("id", "integer", false);
            col.comment = Some("Primary key".to_string());
            let table = make_table(vec![col], None);

            let ddl = adapter.generate_ddl(&table);

            assert!(
                ddl.contains(
                    "COMMENT ON COLUMN \"public\".\"test_table\".\"id\" IS 'Primary key';"
                )
            );
        }

        #[test]
        fn single_quote_in_comment_is_escaped() {
            let adapter = PostgresAdapter::new();
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("It's a test".to_string());

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("IS 'It''s a test';"));
        }

        #[test]
        fn no_comment_on_when_absent() {
            let adapter = PostgresAdapter::new();
            let table = make_table(vec![make_column("id", "integer", false)], None);

            let ddl = adapter.generate_ddl(&table);

            assert!(!ddl.contains("COMMENT ON"));
        }

        #[test]
        fn default_ddl_line_count_matches_generated_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(vec![make_column("col", "text", true)], None);

            let ddl = adapter.generate_ddl(&table);
            let count = adapter.ddl_line_count(&table);

            assert_eq!(count, ddl.lines().count());
        }
    }

    mod sql_dialect_update {
        use super::*;
        use crate::app::ports::SqlDialect;

        #[test]
        fn single_pk_returns_escaped_sql() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "public",
                "users",
                "name",
                "O'Reilly",
                &[("id".into(), "42".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
            );
        }

        #[test]
        fn composite_pk_returns_where_with_all_keys() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "s",
                "t",
                "name",
                "new",
                &[("id".into(), "1".into()), ("tenant_id".into(), "7".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"s\".\"t\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }
    }

    mod sql_dialect_bulk_delete {
        use super::*;
        use crate::app::ports::SqlDialect;

        #[test]
        fn single_pk_single_row_returns_in_clause() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "1".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE \"id\" IN ('1');"
            );
        }

        #[test]
        fn single_pk_multiple_rows_returns_in_clause_with_all_values() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![("id".to_string(), "1".to_string())],
                vec![("id".to_string(), "2".to_string())],
                vec![("id".to_string(), "3".to_string())],
            ];

            let sql = adapter.build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE \"id\" IN ('1', '2', '3');"
            );
        }

        #[test]
        fn composite_pk_returns_row_constructor_in_clause() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![
                    ("id".to_string(), "1".to_string()),
                    ("tenant_id".to_string(), "a".to_string()),
                ],
                vec![
                    ("id".to_string(), "2".to_string()),
                    ("tenant_id".to_string(), "b".to_string()),
                ],
            ];

            let sql = adapter.build_bulk_delete_sql("s", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"s\".\"t\"\nWHERE (\"id\", \"tenant_id\") IN (('1', 'a'), ('2', 'b'));"
            );
        }

        #[test]
        fn null_pk_value_uses_null_literal() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "NULL".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IN (NULL);");
        }

        #[test]
        fn pk_value_with_quotes_is_escaped() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "O'Reilly".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IN ('O''Reilly');"
            );
        }
    }
}
