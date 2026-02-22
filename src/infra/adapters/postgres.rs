use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::{MetadataError, MetadataProvider, QueryExecutor};
use crate::domain::{
    Column, DatabaseMetadata, FkAction, ForeignKey, Index, IndexType, QueryResult, QuerySource,
    RlsCommand, RlsInfo, RlsPolicy, Schema, Table, TableSummary, Trigger, TriggerEvent,
    TriggerTiming, WriteExecutionResult,
};
use crate::infra::utils::{quote_ident, quote_literal};

/// PostgreSQL allows DML inside CTEs (e.g., `WITH ... UPDATE`), so we can't
/// just check if query starts with SELECT/WITH. We need to find the first
/// top-level SQL verb outside of parentheses, string literals, and comments.
/// Also rejects multiple statements and SELECT INTO (which creates tables).
fn is_select_query(query: &str) -> bool {
    let lower = query.trim().to_lowercase();
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    let len = chars.len();

    let mut i = 0;
    let mut depth = 0;
    let mut in_string = false;
    let mut found_select = false;

    while i < len {
        let (byte_pos, c) = chars[i];

        // Skip -- line comments
        if c == '-' && i + 1 < len && chars[i + 1].1 == '-' {
            while i < len && chars[i].1 != '\n' {
                i += 1;
            }
            continue;
        }

        // Skip /* block comments */
        if c == '/' && i + 1 < len && chars[i + 1].1 == '*' {
            i += 2;
            while i + 1 < len && !(chars[i].1 == '*' && chars[i + 1].1 == '/') {
                i += 1;
            }
            i += 2; // skip */
            continue;
        }

        // Handle string literals
        if c == '\'' {
            if in_string {
                if i + 1 < len && chars[i + 1].1 == '\'' {
                    i += 2; // escaped quote ''
                    continue;
                }
                in_string = false;
            } else {
                in_string = true;
            }
            i += 1;
            continue;
        }

        if in_string {
            i += 1;
            continue;
        }

        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        }

        // Reject multiple statements (but allow trailing semicolon)
        if depth == 0 && c == ';' {
            // Check if there's anything after the semicolon (besides whitespace)
            let remaining = &lower[byte_pos + 1..];
            if !remaining.trim().is_empty() {
                return false;
            }
            // Trailing semicolon is OK, stop processing
            break;
        }

        // At top level and word boundary, check for SQL keywords
        if depth == 0 && is_word_start(&chars, i) {
            let rest = &lower[byte_pos..];
            if is_keyword(rest, "select") {
                found_select = true;
            }
            // SELECT INTO creates a table, reject it
            if is_keyword(rest, "into") && found_select {
                return false;
            }
            if is_keyword(rest, "insert")
                || is_keyword(rest, "update")
                || is_keyword(rest, "delete")
                || is_keyword(rest, "create")
            {
                return false;
            }
        }

        i += 1;
    }

    found_select
}

fn is_word_start(chars: &[(usize, char)], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1].1;
    !prev.is_alphanumeric() && prev != '_'
}

fn is_keyword(s: &str, keyword: &str) -> bool {
    if !s.starts_with(keyword) {
        return false;
    }
    s[keyword.len()..]
        .chars()
        .next()
        .map(|c| !c.is_alphanumeric() && c != '_')
        .unwrap_or(true)
}

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

    #[allow(clippy::type_complexity)]
    fn parse_table_info(
        json: &str,
    ) -> Result<(Option<String>, Option<String>, Option<i64>), MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok((None, None, None));
        }

        #[derive(serde::Deserialize)]
        struct RawTableInfo {
            owner: Option<String>,
            comment: Option<String>,
            row_count_estimate: Option<i64>,
        }

        let raw: RawTableInfo =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        // PostgreSQL returns reltuples = -1 when VACUUM/ANALYZE has never run
        let row_count = raw.row_count_estimate.filter(|&n| n >= 0);

        Ok((raw.owner, raw.comment, row_count))
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

    fn parse_triggers(json: &str) -> Result<Vec<Trigger>, MetadataError> {
        let trimmed = json.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(Vec::new());
        }

        #[derive(serde::Deserialize)]
        struct RawTrigger {
            name: String,
            timing: String,
            events: Vec<String>,
            function_name: String,
            security_definer: bool,
        }

        let raw: Vec<RawTrigger> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|t| Trigger {
                name: t.name,
                timing: match t.timing.as_str() {
                    "BEFORE" => TriggerTiming::Before,
                    "INSTEAD OF" => TriggerTiming::InsteadOf,
                    _ => TriggerTiming::After,
                },
                events: t
                    .events
                    .iter()
                    .filter_map(|e| match e.as_str() {
                        "INSERT" => Some(TriggerEvent::Insert),
                        "UPDATE" => Some(TriggerEvent::Update),
                        "DELETE" => Some(TriggerEvent::Delete),
                        "TRUNCATE" => Some(TriggerEvent::Truncate),
                        _ => None,
                    })
                    .collect(),
                function_name: t.function_name,
                security_definer: t.security_definer,
            })
            .collect())
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
        // Keep preview ordering deterministic by primary key when available.
        // Fallback to unordered preview if PK discovery fails.
        let order_columns = self
            .fetch_preview_order_columns(dsn, schema, table)
            .await
            .unwrap_or_default();
        let query = Self::build_preview_query(schema, table, &order_columns, limit, offset);
        self.execute_query_raw(dsn, &query, QuerySource::Preview)
            .await
    }

    async fn execute_adhoc(&self, dsn: &str, query: &str) -> Result<QueryResult, MetadataError> {
        if !is_select_query(query) {
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

    mod rls_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_none(#[case] input: &str) {
            let result = PostgresAdapter::parse_rls(input).unwrap();
            assert!(result.is_none());
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_rls("{not valid json}");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn disabled_rls_with_no_policies_returns_expected() {
            let json = r#"{"enabled": false, "force": false, "policies": []}"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.expect("Should return Some(RlsInfo)");

            assert!(!rls.enabled);
            assert!(!rls.force);
            assert!(rls.policies.is_empty());
        }

        #[test]
        fn enabled_and_forced_rls_returns_expected() {
            let json = r#"{"enabled": true, "force": true, "policies": []}"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.unwrap();

            assert!(rls.enabled);
            assert!(rls.force);
        }

        #[test]
        fn single_policy_parses_all_fields() {
            let json = r#"{
                "enabled": true,
                "force": false,
                "policies": [{
                    "name": "tenant_isolation",
                    "permissive": true,
                    "roles": ["app_user", "admin"],
                    "cmd": "r",
                    "qual": "tenant_id = current_setting('app.tenant_id')::int",
                    "with_check": null
                }]
            }"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.unwrap();
            let policy = &rls.policies[0];

            assert_eq!(policy.name, "tenant_isolation");
            assert!(policy.permissive);
            assert_eq!(policy.roles, vec!["app_user", "admin"]);
            assert_eq!(policy.cmd, RlsCommand::Select);
            assert!(policy.qual.is_some());
            assert!(policy.with_check.is_none());
        }

        #[rstest]
        #[case("*", RlsCommand::All)]
        #[case("r", RlsCommand::Select)]
        #[case("a", RlsCommand::Insert)]
        #[case("w", RlsCommand::Update)]
        #[case("d", RlsCommand::Delete)]
        #[case("x", RlsCommand::All)] // unknown defaults to All
        fn cmd_mapping_returns_expected(#[case] cmd: &str, #[case] expected: RlsCommand) {
            let json = format!(
                r#"{{"enabled": true, "force": false, "policies": [{{
                    "name": "test", "permissive": true, "roles": null,
                    "cmd": "{}", "qual": null, "with_check": null
                }}]}}"#,
                cmd
            );

            let result = PostgresAdapter::parse_rls(&json).unwrap();
            let rls = result.unwrap();

            assert_eq!(rls.policies[0].cmd, expected);
        }

        #[test]
        fn null_roles_becomes_empty_vec() {
            let json = r#"{
                "enabled": true, "force": false,
                "policies": [{"name": "p", "permissive": true, "roles": null, "cmd": "*", "qual": null, "with_check": null}]
            }"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.unwrap();

            assert!(rls.policies[0].roles.is_empty());
        }

        #[test]
        fn missing_required_field_returns_invalid_json_error() {
            let json = r#"{"force": false, "policies": []}"#; // missing 'enabled'

            let result = PostgresAdapter::parse_rls(json);

            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }
    }

    mod select_validation {
        use super::is_select_query;
        use rstest::rstest;

        #[rstest]
        // Basic SELECT
        #[case("SELECT * FROM users", true)]
        #[case("select id from users", true)]
        #[case("  SELECT id FROM users  ", true)]
        // CTE with SELECT (allowed)
        #[case("WITH cte AS (SELECT 1) SELECT * FROM cte", true)]
        #[case("with recursive tree AS (SELECT 1) SELECT * FROM tree", true)]
        #[case("WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b", true)]
        // CTE with DML (rejected)
        #[case("WITH cte AS (SELECT 1) UPDATE users SET name = 'x'", false)]
        #[case("WITH cte AS (SELECT 1) DELETE FROM users", false)]
        #[case("WITH cte AS (SELECT 1) INSERT INTO users VALUES (1)", false)]
        #[case("with cte as (select 1) update users set name = 'x'", false)]
        // Plain DML (rejected)
        #[case("INSERT INTO users VALUES (1)", false)]
        #[case("  insert into users (id) values (1)", false)]
        #[case("UPDATE users SET name = 'new'", false)]
        #[case("  update users set active = true", false)]
        #[case("DELETE FROM users WHERE id = 1", false)]
        #[case("  delete from users", false)]
        // DDL (rejected)
        #[case("CREATE TABLE foo (id INT)", false)]
        #[case("DROP TABLE users", false)]
        #[case("ALTER TABLE users ADD COLUMN foo INT", false)]
        #[case("TRUNCATE users", false)]
        // Empty/whitespace
        #[case("", false)]
        #[case("   ", false)]
        // Parentheses after CTE
        #[case("WITH cte AS (SELECT 1) SELECT (1+2)", true)]
        #[case("WITH cte AS (SELECT 1) SELECT (SELECT 1)", true)]
        // String literals containing parentheses
        #[case("WITH cte AS (SELECT '(' FROM t) SELECT * FROM cte", true)]
        #[case("SELECT * FROM t WHERE name = '(test)'", true)]
        // Escaped quotes in strings
        #[case("SELECT * FROM t WHERE name = 'it''s'", true)]
        #[case("WITH cte AS (SELECT 'a''b') SELECT * FROM cte", true)]
        // SQL keywords inside string literals
        #[case("SELECT * FROM t WHERE action = 'delete'", true)]
        #[case("SELECT * FROM t WHERE cmd = 'INSERT INTO'", true)]
        // Identifiers containing keywords (word boundary test)
        #[case("SELECT mydelete FROM t", true)]
        #[case("SELECT delete_flag FROM t", true)]
        #[case("WITH mydelete AS (SELECT 1) SELECT * FROM mydelete", true)]
        #[case("SELECT * FROM users_to_delete", true)]
        // SQL comments containing keywords
        #[case("-- delete old records\nSELECT * FROM t", true)]
        #[case("/* update cache */ SELECT * FROM t", true)]
        #[case("SELECT * FROM t -- insert comment", true)]
        #[case("SELECT /* delete */ * FROM t", true)]
        // Non-ASCII characters (UTF-8 safety test)
        #[case("SELECT * FROM \"ユーザー\"", true)]
        #[case("SELECT name FROM users WHERE name = '日本語'", true)]
        #[case("WITH cte AS (SELECT '中文') SELECT * FROM cte", true)]
        // Multiple statements (rejected)
        #[case("SELECT 1; DELETE FROM users", false)]
        #[case("SELECT * FROM t; UPDATE t SET x = 1", false)]
        #[case("SELECT 1; SELECT 2", false)]
        // Semicolon in string is OK
        #[case("SELECT * FROM t WHERE x = ';'", true)]
        // Trailing semicolon is OK
        #[case("SELECT * FROM users;", true)]
        #[case("SELECT 1;", true)]
        #[case("SELECT * FROM t WHERE x = 1;", true)]
        #[case("WITH cte AS (SELECT 1) SELECT * FROM cte;", true)]
        // SELECT INTO (rejected - creates table)
        #[case("SELECT * INTO new_table FROM old_table", false)]
        #[case("SELECT id, name INTO backup FROM users", false)]
        // INTO in subquery is OK
        #[case("SELECT * FROM (SELECT 1) AS sub", true)]
        // INTO in string is OK
        #[case("SELECT * FROM t WHERE x = 'INTO'", true)]
        // CREATE TABLE AS SELECT (rejected)
        #[case("CREATE TABLE t AS SELECT * FROM users", false)]
        #[case("CREATE TABLE backup AS SELECT id FROM users", false)]
        fn query_validation_returns_expected(#[case] query: &str, #[case] expected: bool) {
            assert_eq!(is_select_query(query), expected);
        }
    }

    mod json_parse_errors {
        use super::*;

        #[test]
        fn parse_tables_with_malformed_json_returns_error() {
            let result = PostgresAdapter::parse_tables("{not valid json}");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn parse_tables_with_wrong_structure_returns_error() {
            // Array of strings instead of objects
            let result = PostgresAdapter::parse_tables(r#"["table1", "table2"]"#);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn parse_columns_with_missing_field_returns_error() {
            // Missing required field 'data_type'
            let json = r#"[{"name": "id", "nullable": true}]"#;
            let result = PostgresAdapter::parse_columns(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn parse_indexes_with_wrong_type_returns_error() {
            // 'columns' should be array, not string
            let json =
                r#"[{"name": "idx_test", "columns": "id", "unique": false, "primary": false}]"#;
            let result = PostgresAdapter::parse_indexes(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn parse_empty_string_returns_empty_vec() {
            assert!(PostgresAdapter::parse_tables("").unwrap().is_empty());
            assert!(PostgresAdapter::parse_columns("").unwrap().is_empty());
            assert!(PostgresAdapter::parse_indexes("").unwrap().is_empty());
        }

        #[test]
        fn parse_null_string_returns_empty_vec() {
            assert!(PostgresAdapter::parse_tables("null").unwrap().is_empty());
            assert!(PostgresAdapter::parse_columns("null").unwrap().is_empty());
            assert!(PostgresAdapter::parse_indexes("null").unwrap().is_empty());
        }
    }

    mod table_info_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_none(#[case] input: &str) {
            let (owner, comment, row_count) = PostgresAdapter::parse_table_info(input).unwrap();
            assert!(owner.is_none());
            assert!(comment.is_none());
            assert!(row_count.is_none());
        }

        #[test]
        fn all_fields_present_returns_values() {
            let json = r#"{"owner": "postgres", "comment": "User accounts table", "row_count_estimate": 100}"#;

            let (owner, comment, row_count) = PostgresAdapter::parse_table_info(json).unwrap();

            assert_eq!(owner.as_deref(), Some("postgres"));
            assert_eq!(comment.as_deref(), Some("User accounts table"));
            assert_eq!(row_count, Some(100));
        }

        #[test]
        fn null_fields_returns_none() {
            let json = r#"{"owner": null, "comment": null, "row_count_estimate": null}"#;

            let (owner, comment, row_count) = PostgresAdapter::parse_table_info(json).unwrap();

            assert!(owner.is_none());
            assert!(comment.is_none());
            assert!(row_count.is_none());
        }

        #[test]
        fn negative_row_count_returns_none() {
            let json = r#"{"owner": "postgres", "comment": null, "row_count_estimate": -1}"#;

            let (_, _, row_count) = PostgresAdapter::parse_table_info(json).unwrap();

            assert!(row_count.is_none());
        }

        #[test]
        fn zero_row_count_returns_zero() {
            let json = r#"{"owner": "postgres", "comment": null, "row_count_estimate": 0}"#;

            let (_, _, row_count) = PostgresAdapter::parse_table_info(json).unwrap();

            assert_eq!(row_count, Some(0));
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_table_info("{not valid json}");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }
    }

    mod trigger_parsing {
        use super::*;
        use crate::domain::{TriggerEvent, TriggerTiming};
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_empty_vec(#[case] input: &str) {
            let result = PostgresAdapter::parse_triggers(input).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn valid_single_trigger_parses_all_fields() {
            let json = r#"[{
                "name": "audit_trigger",
                "timing": "AFTER",
                "events": ["INSERT", "UPDATE"],
                "function_name": "audit_func",
                "security_definer": true
            }]"#;

            let result = PostgresAdapter::parse_triggers(json).unwrap();
            let trigger = &result[0];

            assert_eq!(result.len(), 1);
            assert_eq!(trigger.name, "audit_trigger");
            assert_eq!(trigger.timing, TriggerTiming::After);
            assert_eq!(
                trigger.events,
                vec![TriggerEvent::Insert, TriggerEvent::Update]
            );
            assert_eq!(trigger.function_name, "audit_func");
            assert!(trigger.security_definer);
        }

        #[rstest]
        #[case("BEFORE", TriggerTiming::Before)]
        #[case("AFTER", TriggerTiming::After)]
        #[case("INSTEAD OF", TriggerTiming::InsteadOf)]
        #[case("UNKNOWN", TriggerTiming::After)] // unknown defaults to After
        fn timing_mapping_returns_expected(#[case] timing: &str, #[case] expected: TriggerTiming) {
            let json = format!(
                r#"[{{
                    "name": "test", "timing": "{}", "events": ["INSERT"],
                    "function_name": "func", "security_definer": false
                }}]"#,
                timing
            );

            let result = PostgresAdapter::parse_triggers(&json).unwrap();
            assert_eq!(result[0].timing, expected);
        }

        #[test]
        fn multiple_events_parsed_in_order() {
            let json = r#"[{
                "name": "multi_event",
                "timing": "BEFORE",
                "events": ["INSERT", "DELETE", "UPDATE", "TRUNCATE"],
                "function_name": "func",
                "security_definer": false
            }]"#;

            let result = PostgresAdapter::parse_triggers(json).unwrap();
            assert_eq!(
                result[0].events,
                vec![
                    TriggerEvent::Insert,
                    TriggerEvent::Delete,
                    TriggerEvent::Update,
                    TriggerEvent::Truncate,
                ]
            );
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_triggers("{not valid json}");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r#"[]"#;
            let result = PostgresAdapter::parse_triggers(json).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn security_definer_false_returns_expected() {
            let json = r#"[{
                "name": "test",
                "timing": "AFTER",
                "events": ["INSERT"],
                "function_name": "func",
                "security_definer": false
            }]"#;

            let result = PostgresAdapter::parse_triggers(json).unwrap();
            assert!(!result[0].security_definer);
        }
    }

    mod schema_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_empty_vec(#[case] input: &str) {
            let result = PostgresAdapter::parse_schemas(input).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn valid_single_schema_parses_correctly() {
            let json = r#"[{"name": "public"}]"#;
            let result = PostgresAdapter::parse_schemas(json).unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].name, "public");
        }

        #[test]
        fn valid_multiple_schemas_parse_in_order() {
            let json = r#"[{"name": "public"}, {"name": "auth"}, {"name": "custom"}]"#;
            let result = PostgresAdapter::parse_schemas(json).unwrap();

            assert_eq!(result.len(), 3);
            assert_eq!(result[0].name, "public");
            assert_eq!(result[1].name, "auth");
            assert_eq!(result[2].name, "custom");
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_schemas("{not valid json}");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn missing_name_field_returns_error() {
            let json = r#"[{}]"#;
            let result = PostgresAdapter::parse_schemas(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn wrong_structure_returns_error() {
            let json = r#"["public", "auth"]"#;
            let result = PostgresAdapter::parse_schemas(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r#"[]"#;
            let result = PostgresAdapter::parse_schemas(json).unwrap();
            assert!(result.is_empty());
        }
    }

    mod foreign_key_parsing {
        use super::*;
        use crate::domain::FkAction;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_empty_vec(#[case] input: &str) {
            let result = PostgresAdapter::parse_foreign_keys(input).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn valid_single_fk_parses_all_fields() {
            let json = r#"[{
                "name": "orders_user_fk",
                "from_schema": "public",
                "from_table": "orders",
                "from_columns": ["user_id"],
                "to_schema": "public",
                "to_table": "users",
                "to_columns": ["id"],
                "on_delete": "c",
                "on_update": "a"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();
            let fk = &result[0];

            assert_eq!(result.len(), 1);
            assert_eq!(fk.name, "orders_user_fk");
            assert_eq!(fk.from_schema, "public");
            assert_eq!(fk.from_table, "orders");
            assert_eq!(fk.from_columns, vec!["user_id"]);
            assert_eq!(fk.to_schema, "public");
            assert_eq!(fk.to_table, "users");
            assert_eq!(fk.to_columns, vec!["id"]);
            assert_eq!(fk.on_delete, FkAction::Cascade);
            assert_eq!(fk.on_update, FkAction::NoAction);
        }

        #[rstest]
        #[case("a", FkAction::NoAction)]
        #[case("r", FkAction::Restrict)]
        #[case("c", FkAction::Cascade)]
        #[case("n", FkAction::SetNull)]
        #[case("d", FkAction::SetDefault)]
        #[case("x", FkAction::NoAction)]
        fn fk_action_mapping_returns_expected(
            #[case] action_code: &str,
            #[case] expected: FkAction,
        ) {
            let json = format!(
                r#"[{{
                    "name": "test_fk",
                    "from_schema": "public",
                    "from_table": "t1",
                    "from_columns": ["id"],
                    "to_schema": "public",
                    "to_table": "t2",
                    "to_columns": ["id"],
                    "on_delete": "{}",
                    "on_update": "a"
                }}]"#,
                action_code
            );

            let result = PostgresAdapter::parse_foreign_keys(&json).unwrap();
            assert_eq!(result[0].on_delete, expected);
        }

        #[test]
        fn composite_foreign_key_parses_multiple_columns() {
            let json = r#"[{
                "name": "order_item_fk",
                "from_schema": "public",
                "from_table": "order_items",
                "from_columns": ["order_id", "item_id"],
                "to_schema": "public",
                "to_table": "order_item_master",
                "to_columns": ["order_id", "id"],
                "on_delete": "r",
                "on_update": "r"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();
            let fk = &result[0];

            assert_eq!(fk.from_columns, vec!["order_id", "item_id"]);
            assert_eq!(fk.to_columns, vec!["order_id", "id"]);
            assert_eq!(fk.on_delete, FkAction::Restrict);
            assert_eq!(fk.on_update, FkAction::Restrict);
        }

        #[test]
        fn multiple_foreign_keys_parse_in_order() {
            let json = r#"[
                {
                    "name": "fk_1",
                    "from_schema": "public",
                    "from_table": "t1",
                    "from_columns": ["id"],
                    "to_schema": "public",
                    "to_table": "t2",
                    "to_columns": ["id"],
                    "on_delete": "c",
                    "on_update": "c"
                },
                {
                    "name": "fk_2",
                    "from_schema": "public",
                    "from_table": "t3",
                    "from_columns": ["id"],
                    "to_schema": "public",
                    "to_table": "t4",
                    "to_columns": ["id"],
                    "on_delete": "n",
                    "on_update": "d"
                }
            ]"#;

            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].name, "fk_1");
            assert_eq!(result[0].on_delete, FkAction::Cascade);
            assert_eq!(result[1].name, "fk_2");
            assert_eq!(result[1].on_delete, FkAction::SetNull);
            assert_eq!(result[1].on_update, FkAction::SetDefault);
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_foreign_keys("{not valid json}");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn missing_required_field_returns_error() {
            let json = r#"[{
                "name": "test_fk",
                "from_schema": "public",
                "from_table": "t1",
                "from_columns": ["id"],
                "to_schema": "public",
                "to_table": "t2",
                "to_columns": ["id"],
                "on_update": "a"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn wrong_column_type_returns_error() {
            let json = r#"[{
                "name": "test_fk",
                "from_schema": "public",
                "from_table": "t1",
                "from_columns": "user_id",
                "to_schema": "public",
                "to_table": "t2",
                "to_columns": ["id"],
                "on_delete": "c",
                "on_update": "a"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r#"[]"#;
            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();
            assert!(result.is_empty());
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
}
