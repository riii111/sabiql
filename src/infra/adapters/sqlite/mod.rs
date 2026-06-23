use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider, QueryExecutor};
use crate::domain::{
    Column, ColumnAttributes, CommandTag, DatabaseMetadata, FkAction, ForeignKey, Index,
    IndexAttributes, IndexType, QueryResult, QuerySource, QueryValue, Schema, Table,
    TableSignature, TableSummary, WriteExecutionResult,
};

mod cli;
mod sql;

use cli::SqliteCli;

const MAIN_SCHEMA: &str = "main";

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    cli: SqliteCli,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTable {
    name: String,
    sql: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawColumn {
    cid: i32,
    name: String,
    #[serde(rename = "type")]
    data_type: String,
    notnull: i64,
    dflt_value: Option<String>,
    pk: i64,
    #[serde(default)]
    hidden: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawIndex {
    name: String,
    unique: i64,
    origin: String,
    #[serde(default)]
    partial: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawIndexColumn {
    seqno: i64,
    cid: i64,
    name: Option<String>,
    key: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSql {
    sql: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawForeignKey {
    id: i64,
    seq: i64,
    table: String,
    from: String,
    to: Option<String>,
    on_update: String,
    on_delete: String,
}

#[derive(Debug, Clone, Deserialize)]
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

    async fn has_virtual_tables(&self, path: &str) -> Result<bool, DbOperationError> {
        let rows: Vec<RawRowCount> = self
            .cli
            .execute_json(path, sql::has_virtual_tables_query())
            .await?;
        Ok(rows.into_iter().next().is_some_and(|row| row.count > 0))
    }

    async fn list_tables(&self, path: &str) -> Result<Vec<RawTable>, DbOperationError> {
        match self.cli.execute_json(path, sql::user_tables_query()).await {
            Ok(tables) => Ok(tables),
            Err(DbOperationError::QueryFailed(message))
                if sql::is_table_list_unavailable(&message) =>
            {
                if self.has_virtual_tables(path).await? {
                    return Err(sql::table_list_required_error());
                }
                self.cli
                    .execute_json(path, sql::legacy_user_tables_query())
                    .await
            }
            Err(error) => Err(error),
        }
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

    async fn table_definition(&self, path: &str, table: &str) -> Option<String> {
        let rows: Result<Vec<RawSql>, DbOperationError> = self
            .cli
            .execute_json(path, &sql::table_definition_query(table))
            .await;
        rows.ok()
            .and_then(|rows| rows.into_iter().next())
            .and_then(|row| row.sql)
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

    fn preview_visible_column_names(raw_columns: Vec<RawColumn>) -> Vec<String> {
        raw_columns
            .into_iter()
            .filter(|column| column.hidden != 1)
            .map(|column| column.name)
            .collect()
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

    fn validate_main_schema(schema: &str) -> Result<(), DbOperationError> {
        if schema == MAIN_SCHEMA {
            Ok(())
        } else {
            Err(DbOperationError::ObjectMissing(format!(
                "SQLite schema not found: {schema}"
            )))
        }
    }

    async fn preview_order_columns(&self, path: &str, table: &str) -> Vec<String> {
        self.primary_key_columns(path, table)
            .await
            .unwrap_or_default()
    }

    async fn execute_quoted_query(
        &self,
        path: &str,
        query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        self.execute_quoted_query_with_display_query(path, query, query, source, read_only)
            .await
    }

    async fn execute_quoted_query_with_display_query(
        &self,
        path: &str,
        execution_query: &str,
        display_query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self
            .cli
            .execute_quote(path, execution_query, read_only)
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let mut result = quoted_to_query_result(execution_query, &stdout, source, elapsed)?;
        result.query = display_query.to_string();
        Ok(result)
    }

    async fn execute_changes_query(
        &self,
        path: &str,
        query: &str,
        read_only: bool,
    ) -> Result<(usize, u64), DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self
            .cli
            .execute_csv(path, &append_changes_query(query), read_only)
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        Ok((parse_affected_rows(&stdout)?, elapsed))
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
                .execute_json(path, &sql::index_xinfo_query(&raw.name))
                .await?;
            columns.sort_by_key(|col| col.seqno);
            let has_expression = columns.iter().any(|col| col.key != 0 && col.cid == -2);
            let has_auxiliary_columns = columns.iter().any(|col| col.key == 0);
            let columns = Self::index_key_column_names(&columns);
            let definition = self.index_definition(path, &raw.name).await;

            let mut attributes = IndexAttributes::from_parts(raw.unique != 0, raw.origin == "pk");
            if raw.partial != 0 {
                attributes = attributes | IndexAttributes::PARTIAL;
            }
            if has_expression {
                attributes = attributes | IndexAttributes::EXPRESSION;
            }
            if has_auxiliary_columns {
                attributes = attributes | IndexAttributes::HAS_AUXILIARY_COLUMNS;
            }

            indexes.push(Index {
                name: raw.name,
                columns,
                attributes,
                index_type: IndexType::Unknown,
                definition,
            });
        }

        indexes.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(indexes)
    }

    async fn unique_single_columns(
        &self,
        path: &str,
        table: &str,
    ) -> Result<std::collections::HashSet<String>, DbOperationError> {
        let raw_indexes: Vec<RawIndex> = self
            .cli
            .execute_json(path, &sql::index_list_query(table))
            .await?;
        let mut columns = std::collections::HashSet::new();

        for raw in raw_indexes
            .into_iter()
            .filter(|index| index.unique != 0 && index.partial == 0)
        {
            let key_columns = self.index_key_columns(path, &raw.name).await?;
            if key_columns.len() == 1 && key_columns[0] != "<expression>" {
                columns.insert(key_columns[0].clone());
            }
        }

        Ok(columns)
    }

    async fn index_key_columns(
        &self,
        path: &str,
        index: &str,
    ) -> Result<Vec<String>, DbOperationError> {
        let mut columns: Vec<RawIndexColumn> = self
            .cli
            .execute_json(path, &sql::index_xinfo_query(index))
            .await?;
        columns.sort_by_key(|col| col.seqno);
        Ok(Self::index_key_column_names(&columns))
    }

    fn index_key_column_names(columns: &[RawIndexColumn]) -> Vec<String> {
        columns
            .iter()
            .filter(|col| col.key != 0)
            .map(|col| {
                if col.cid == -2 {
                    "<expression>".to_string()
                } else {
                    col.name.clone().unwrap_or_else(|| "<unknown>".to_string())
                }
            })
            .collect()
    }

    async fn index_definition(&self, path: &str, index: &str) -> Option<String> {
        let rows: Result<Vec<RawSql>, DbOperationError> = self
            .cli
            .execute_json(path, &sql::index_definition_query(index))
            .await;
        rows.ok()
            .and_then(|rows| rows.into_iter().next())
            .and_then(|row| row.sql)
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
            let to_column = if let Some(to) = fk.to {
                to
            } else {
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
        let (indexes, unique_single_columns) = if include_indexes {
            let indexes = self.indexes(path, table).await?;
            let unique_single_columns = indexes
                .iter()
                .filter(|index| {
                    index.is_unique()
                        && !index.is_partial()
                        && !index.has_expression()
                        && index.columns.len() == 1
                })
                .map(|index| index.columns[0].clone())
                .collect::<std::collections::HashSet<_>>();
            (indexes, unique_single_columns)
        } else {
            (Vec::new(), self.unique_single_columns(path, table).await?)
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
                let is_hidden = column.hidden == 1;
                let is_generated = column.hidden == 2 || column.hidden == 3;
                let is_read_only = is_hidden || is_generated;
                let mut attributes = ColumnAttributes::from_parts(
                    column.notnull == 0 && !is_pk,
                    is_pk,
                    unique_single_columns.contains(column.name.as_str()),
                );
                if is_read_only {
                    attributes = attributes | ColumnAttributes::READ_ONLY;
                }
                if is_hidden {
                    attributes = attributes | ColumnAttributes::HIDDEN;
                }
                if is_generated {
                    attributes = attributes | ColumnAttributes::GENERATED;
                }

                Column {
                    name: column.name.clone(),
                    data_type: column.data_type,
                    default: column.dflt_value,
                    attributes,
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
            source_ddl: self.table_definition(path, table).await,
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
                "col={}:{}:{}:{}:{}:{}:{}",
                column.name,
                column.data_type,
                column.is_nullable(),
                column.default.clone().unwrap_or_default(),
                column.is_read_only(),
                column.is_hidden(),
                column.is_generated()
            )
        }));
        parts.extend(detail.indexes.iter().map(|index| {
            format!(
                "idx={}:{}:{}:{}:{}:{}:{}:{}",
                index.name,
                index.columns.join(","),
                index.is_unique(),
                index.is_primary(),
                index.is_partial(),
                index.has_expression(),
                index.has_auxiliary_columns(),
                index.definition.clone().unwrap_or_default()
            )
        }));
        parts.extend(detail.foreign_keys.iter().map(|fk| {
            format!(
                "fk={}:{}:{}:{}:{}:{}",
                fk.name,
                fk.from_columns.join(","),
                fk.to_table,
                fk.to_columns.join(","),
                fk.on_delete,
                fk.on_update
            )
        }));

        Ok(TableSignature {
            schema: MAIN_SCHEMA.to_string(),
            name: table.name.clone(),
            signature: parts.join("|"),
        })
    }
}

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
            } else {
                return i + 1;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_bracket_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b']' {
            return i + 1;
        }
        i += 1;
    }
    i
}

/// Returns the next SQL keyword and the byte offset immediately after it.
fn next_keyword_from(sql: &str, mut i: usize) -> Option<(&str, usize)> {
    let bytes = sql.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b'\'' | b'"' | b'`' => {
                i = skip_quoted(bytes, i, bytes[i]);
            }
            b'[' => {
                i = skip_bracket_quoted(bytes, i);
            }
            b if b.is_ascii_alphabetic() => {
                let start = i;
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
                return Some((&sql[start..i], i));
            }
            _ => i += 1,
        }
    }
    None
}

fn first_keyword(sql: &str) -> &str {
    next_keyword_from(sql, 0).map_or("", |(keyword, _)| keyword)
}

fn second_keyword(sql: &str) -> Option<&str> {
    let (_, end) = next_keyword_from(sql, 0)?;
    next_keyword_from(sql, end).map(|(keyword, _)| keyword)
}

fn third_keyword(sql: &str) -> Option<&str> {
    let (_, first_end) = next_keyword_from(sql, 0)?;
    let (_, second_end) = next_keyword_from(sql, first_end)?;
    next_keyword_from(sql, second_end).map(|(keyword, _)| keyword)
}

fn fourth_keyword(sql: &str) -> Option<&str> {
    let (_, first_end) = next_keyword_from(sql, 0)?;
    let (_, second_end) = next_keyword_from(sql, first_end)?;
    let (_, third_end) = next_keyword_from(sql, second_end)?;
    next_keyword_from(sql, third_end).map(|(keyword, _)| keyword)
}

fn contains_keyword(sql: &str, expected: &str) -> bool {
    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(sql, offset) {
        if keyword.eq_ignore_ascii_case(expected) {
            return true;
        }
        offset = end;
    }
    false
}

fn split_sqlite_statements(sql: &str) -> Vec<&str> {
    let bytes = sql.as_bytes();
    let mut statements = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b'\'' | b'"' | b'`' => {
                i = skip_quoted(bytes, i, bytes[i]);
            }
            b'[' => {
                i = skip_bracket_quoted(bytes, i);
            }
            b';' => {
                let statement = sql[start..i].trim();
                if !statement.is_empty() {
                    statements.push(statement);
                }
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }

    let tail = sql[start..].trim();
    if !tail.is_empty() {
        statements.push(tail);
    }

    statements
}

fn is_transaction_control(statement: &str) -> bool {
    matches!(
        first_keyword(statement).to_ascii_uppercase().as_str(),
        "BEGIN" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE"
    )
}

fn is_write_statement(statement: &str) -> bool {
    matches!(
        first_keyword(statement).to_ascii_uppercase().as_str(),
        "INSERT" | "REPLACE" | "UPDATE" | "DELETE" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE"
    )
}

use crate::app::policy::sql::sqlite_export::is_sqlite_rerunnable_export_query;

fn sqlite_export_not_rerunnable_error() -> DbOperationError {
    DbOperationError::UnsupportedOperation(
        "Cannot re-execute this query for CSV export because it contains write or DDL statements"
            .to_string(),
    )
}

fn should_wrap_transaction(query: &str) -> bool {
    let statements = split_sqlite_statements(query);
    statements.len() > 1
        && statements.iter().any(|stmt| is_write_statement(stmt))
        && !statements.iter().any(|stmt| is_transaction_control(stmt))
}

fn sqlite_transaction_block(query: &str) -> String {
    let trimmed = query.trim_end().trim_end_matches(';').trim_end();
    format!("BEGIN;\n{trimmed}\n;\nCOMMIT")
}

fn sqlite_execution_query(query: &str) -> Cow<'_, str> {
    if should_wrap_transaction(query) {
        Cow::Owned(sqlite_transaction_block(query))
    } else {
        Cow::Borrowed(query)
    }
}

fn sqlite_probe_marker() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "__sabiql_sqlite_probe_{}_{}_{}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

fn sqlite_probe_columns(marker: &str) -> (String, String) {
    (format!("{marker}_stmt"), format!("{marker}_changes"))
}

fn sqlite_changes_probe(marker: &str, index: usize) -> String {
    let (stmt_col, changes_col) = sqlite_probe_columns(marker);
    format!("SELECT {index} AS \"{stmt_col}\", changes() AS \"{changes_col}\"")
}

fn sqlite_result_probe_columns(marker: &str) -> (String, String) {
    (
        format!("{marker}_result_stmt"),
        format!("{marker}_result_marker"),
    )
}

fn sqlite_result_probe(marker: &str, index: usize) -> String {
    let (stmt_col, marker_col) = sqlite_result_probe_columns(marker);
    format!("SELECT {index} AS \"{stmt_col}\", '{marker}' AS \"{marker_col}\"")
}

fn sqlite_adhoc_execution_query(query: &str, marker: &str) -> String {
    let statements = split_sqlite_statements(query);
    if statements.is_empty() {
        return query.to_string();
    }

    let wrap = should_wrap_transaction(query);
    let mut parts = Vec::with_capacity(statements.len() * 2 + usize::from(wrap) * 2);
    if wrap {
        parts.push("BEGIN".to_string());
    }
    for (index, statement) in statements.iter().enumerate() {
        parts.push((*statement).to_string());
        if is_dml_statement(statement) {
            parts.push(sqlite_changes_probe(marker, index));
        }
        if statement_emits_result_set(statement) {
            parts.push(sqlite_result_probe(marker, index));
        }
    }
    if wrap {
        parts.push("COMMIT".to_string());
    }
    parts.join("\n;\n")
}

fn append_changes_query(query: &str) -> String {
    let body = sqlite_execution_query(query).trim_end().to_string();
    // The standalone separator also terminates a trailing line comment before
    // appending the changes() probe.
    format!("{body}\n;\nSELECT changes() AS affected_rows;")
}

fn dml_keyword(statement: &str) -> Option<&'static str> {
    let keyword = first_keyword(statement);
    if keyword.eq_ignore_ascii_case("INSERT") {
        return Some("INSERT");
    }
    if keyword.eq_ignore_ascii_case("REPLACE") {
        return Some("INSERT");
    }
    if keyword.eq_ignore_ascii_case("UPDATE") {
        return Some("UPDATE");
    }
    if keyword.eq_ignore_ascii_case("DELETE") {
        return Some("DELETE");
    }
    if !keyword.eq_ignore_ascii_case("WITH") {
        return None;
    }

    let mut offset = 0;
    while let Some((keyword, end)) = next_keyword_from(statement, offset) {
        if keyword.eq_ignore_ascii_case("INSERT") {
            return Some("INSERT");
        }
        if keyword.eq_ignore_ascii_case("REPLACE") {
            return Some("INSERT");
        }
        if keyword.eq_ignore_ascii_case("UPDATE") {
            return Some("UPDATE");
        }
        if keyword.eq_ignore_ascii_case("DELETE") {
            return Some("DELETE");
        }
        offset = end;
    }
    None
}

fn is_dml_statement(statement: &str) -> bool {
    dml_keyword(statement).is_some()
}

fn statement_emits_result_set(statement: &str) -> bool {
    let keyword = first_keyword(statement);
    if keyword.eq_ignore_ascii_case("SELECT")
        || keyword.eq_ignore_ascii_case("PRAGMA")
        || keyword.eq_ignore_ascii_case("EXPLAIN")
        || keyword.eq_ignore_ascii_case("VALUES")
    {
        return true;
    }
    if is_dml_statement(statement) {
        return contains_keyword(statement, "RETURNING");
    }
    keyword.eq_ignore_ascii_case("WITH")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuotedRecord {
    offset: usize,
    values: Vec<QueryValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SplitSegment {
    offset: usize,
    text: String,
}

fn split_outside_sqlite_quotes(
    input: &str,
    delimiter: u8,
) -> Result<Vec<SplitSegment>, DbOperationError> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut in_quote = false;
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if in_quote && i + 1 < bytes.len() && bytes[i + 1] == b'\'' => {
                i += 2;
            }
            b'\'' => {
                in_quote = !in_quote;
                i += 1;
            }
            byte if byte == delimiter && !in_quote => {
                segments.push(SplitSegment {
                    offset: start,
                    text: input[start..i].to_string(),
                });
                start = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if in_quote {
        return Err(DbOperationError::MetadataParseFailed(
            "unterminated SQLite quoted output".to_string(),
        ));
    }
    segments.push(SplitSegment {
        offset: start,
        text: input[start..].to_string(),
    });
    Ok(segments)
}

fn split_quoted_records(stdout: &str) -> Result<Vec<SplitSegment>, DbOperationError> {
    let mut records = split_outside_sqlite_quotes(stdout, b'\n')?
        .into_iter()
        .map(|segment| SplitSegment {
            offset: segment.offset,
            text: segment.text.trim_end_matches('\r').to_string(),
        })
        .collect::<Vec<_>>();
    records.retain(|segment| !segment.text.is_empty());
    Ok(records)
}

fn split_quoted_fields(record: &str) -> Result<Vec<String>, DbOperationError> {
    split_outside_sqlite_quotes(record, b',')
        .map(|segments| segments.into_iter().map(|segment| segment.text).collect())
}

fn unquote_sql_text(value: &str) -> String {
    value[1..value.len() - 1].replace("''", "'")
}

fn decode_hex_text(hex: &str) -> Result<String, DbOperationError> {
    let bytes = decode_hex_bytes(hex)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_unistr_inner_sql_escapes(value: &str) -> Result<String, DbOperationError> {
    let inner = value
        .strip_prefix("unistr(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| {
            DbOperationError::MetadataParseFailed("invalid SQLite unistr literal".to_string())
        })?;
    let inner = inner
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .ok_or_else(|| {
            DbOperationError::MetadataParseFailed("invalid SQLite unistr literal".to_string())
        })?;

    let mut decoded = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                let next = chars.next().ok_or_else(|| {
                    DbOperationError::MetadataParseFailed(
                        "invalid SQLite unistr SQL string quote".to_string(),
                    )
                })?;
                if next != '\'' {
                    return Err(DbOperationError::MetadataParseFailed(
                        "invalid SQLite unistr SQL string quote".to_string(),
                    ));
                }
                decoded.push('\'');
            }
            '\\' => {
                let next = chars.next().ok_or_else(|| {
                    DbOperationError::MetadataParseFailed(
                        "invalid SQLite unistr escape sequence".to_string(),
                    )
                })?;
                if next == '\\' {
                    decoded.push('\\');
                } else {
                    decoded.push('\\');
                    decoded.push(next);
                }
            }
            ch => decoded.push(ch),
        }
    }
    Ok(decoded)
}

fn decode_sqlite_nul_text_transport(text: &str) -> Result<Option<String>, DbOperationError> {
    if let Some(hex) = text.strip_prefix(&sql::sqlite_nul_text_sentinel()) {
        return decode_hex_text(hex).map(Some);
    }
    if let Some(hex) = text.strip_prefix(sql::PREVIEW_TRANSPORT_UNISTR_PREFIX) {
        return decode_hex_text(hex).map(Some);
    }
    Ok(None)
}

fn decode_preview_transport_unistr(value: &str) -> Result<Option<String>, DbOperationError> {
    let inner = parse_unistr_inner_sql_escapes(value)?;
    decode_sqlite_nul_text_transport(&inner)
}

fn decode_hex_bytes(hex: &str) -> Result<Vec<u8>, DbOperationError> {
    if !hex.len().is_multiple_of(2) {
        return Err(DbOperationError::MetadataParseFailed(
            "invalid SQLite BLOB hex literal".to_string(),
        ));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut chars = hex.as_bytes().chunks_exact(2);
    for pair in &mut chars {
        let raw = std::str::from_utf8(pair)
            .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
        let byte = u8::from_str_radix(raw, 16)
            .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn parse_quoted_value(
    value: &str,
    source: QuerySource,
    decode_preview_transport: bool,
) -> Result<QueryValue, DbOperationError> {
    if value == "NULL" {
        return Ok(QueryValue::Null);
    }
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        let text = unquote_sql_text(value);
        if source == QuerySource::Preview
            && decode_preview_transport
            && let Some(decoded) = decode_sqlite_nul_text_transport(&text)?
        {
            return Ok(QueryValue::Text(decoded));
        }
        return Ok(QueryValue::Text(text));
    }
    if value.starts_with("unistr(") && value.ends_with(')') {
        if source == QuerySource::Preview
            && decode_preview_transport
            && let Some(text) = decode_preview_transport_unistr(value)?
        {
            return Ok(QueryValue::Text(text));
        }
        return Ok(QueryValue::SqlLiteral(value.to_string()));
    }
    if value.len() >= 3
        && value.as_bytes()[1] == b'\''
        && value.ends_with('\'')
        && value.as_bytes()[0].eq_ignore_ascii_case(&b'X')
    {
        return Ok(QueryValue::Blob(decode_hex_bytes(
            &value[2..value.len() - 1],
        )?));
    }
    if value == "Inf" {
        return Ok(QueryValue::SqlLiteral("1e999".to_string()));
    }
    if value == "-Inf" {
        return Ok(QueryValue::SqlLiteral("-1e999".to_string()));
    }
    Ok(QueryValue::SqlLiteral(value.to_string()))
}

fn parse_quoted_records(
    stdout: &str,
    source: QuerySource,
) -> Result<Vec<QuotedRecord>, DbOperationError> {
    split_quoted_records(stdout)?
        .into_iter()
        .enumerate()
        .map(|(index, segment)| {
            let decode_preview_transport = source == QuerySource::Preview && index > 0;
            split_quoted_fields(&segment.text)?
                .into_iter()
                .map(|field| parse_quoted_value(&field, source, decode_preview_transport))
                .collect::<Result<Vec<_>, _>>()
                .map(|values| QuotedRecord {
                    offset: segment.offset,
                    values,
                })
        })
        .collect()
}

fn statement_counts_as_select_tag(statement: &str) -> bool {
    let keyword = first_keyword(statement);
    keyword.eq_ignore_ascii_case("SELECT") || keyword.eq_ignore_ascii_case("WITH")
}

fn quoted_to_query_result(
    query: &str,
    stdout: &str,
    source: QuerySource,
    execution_time_ms: u64,
) -> Result<QueryResult, DbOperationError> {
    let stdout = stdout.trim();
    if stdout.is_empty() {
        return Ok(QueryResult::success(
            query.to_string(),
            Vec::new(),
            Vec::new(),
            execution_time_ms,
            source,
        ));
    }

    let mut records = parse_quoted_records(stdout, source)?;
    let Some(header) = records.first() else {
        return Ok(QueryResult::success(
            query.to_string(),
            Vec::new(),
            Vec::new(),
            execution_time_ms,
            source,
        ));
    };
    let columns = header
        .values
        .iter()
        .map(QueryValue::display_value)
        .collect();
    let values = records.drain(1..).map(|record| record.values).collect();
    Ok(QueryResult::success_with_values(
        query.to_string(),
        columns,
        values,
        execution_time_ms,
        source,
    ))
}

fn last_sqlite_result_set(stdout: &str, marker: &str) -> Result<Option<String>, DbOperationError> {
    let (stmt_col, marker_col) = sqlite_result_probe_columns(marker);
    let raw_records = split_quoted_records(stdout)?;
    let records = parse_quoted_records(stdout, QuerySource::Adhoc)?;

    let mut last_result = None;
    let mut result_start = 0;
    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        if record.values.len() == 2
            && record.values[0].as_str() == Some(stmt_col.as_str())
            && record.values[1].as_str() == Some(marker_col.as_str())
        {
            let value = records.get(index + 1).ok_or_else(|| {
                DbOperationError::CommandTagParseFailed(
                    "missing SQLite result marker row".to_string(),
                )
            })?;
            let marker_value = value
                .values
                .get(1)
                .and_then(QueryValue::as_str)
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite result marker".to_string(),
                    )
                })?;
            if marker_value != marker {
                return Err(DbOperationError::CommandTagParseFailed(
                    "mismatched SQLite result marker".to_string(),
                ));
            }
            last_result = Some(
                raw_records[result_start..index]
                    .iter()
                    .map(|segment| segment.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            index += 2;
            result_start = index;
        } else {
            index += 1;
        }
    }

    Ok(last_result)
}

fn strip_sqlite_probes(
    stdout: &str,
    marker: &str,
) -> Result<(String, HashMap<usize, usize>), DbOperationError> {
    if stdout.trim().is_empty() {
        return Ok((String::new(), HashMap::new()));
    }

    let (stmt_col, changes_col) = sqlite_probe_columns(marker);
    let raw_records = split_quoted_records(stdout)?;
    let records = parse_quoted_records(stdout, QuerySource::Adhoc)?;

    let mut changes = HashMap::new();
    let mut kept = Vec::new();
    let mut removed_probe = false;
    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        if record.values.len() == 2
            && record.values[0].as_str() == Some(stmt_col.as_str())
            && record.values[1].as_str() == Some(changes_col.as_str())
        {
            removed_probe = true;
            let value = records.get(index + 1).ok_or_else(|| {
                DbOperationError::CommandTagParseFailed(
                    "missing SQLite statement probe row".to_string(),
                )
            })?;
            let stmt_index = value
                .values
                .first()
                .and_then(QueryValue::as_str)
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite statement probe index".to_string(),
                    )
                })?;
            let affected_rows = value
                .values
                .get(1)
                .and_then(QueryValue::as_str)
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite statement probe changes".to_string(),
                    )
                })?;
            changes.insert(stmt_index, affected_rows);
            index += 2;
        } else {
            kept.push(raw_records[index].text.clone());
            index += 1;
        }
    }

    if !removed_probe {
        return Ok((stdout.to_string(), changes));
    }

    Ok((kept.join("\n"), changes))
}

fn first_csv_cell(stdout: &str) -> Result<String, DbOperationError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(stdout.trim().as_bytes());
    let mut records = reader.records();
    let record = records
        .next()
        .transpose()?
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))?;
    record
        .get(0)
        .map(ToString::to_string)
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))
}

fn last_csv_cell(stdout: &str) -> Result<String, DbOperationError> {
    let line = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(line.as_bytes());
    let mut records = reader.records();
    let record = records
        .next()
        .transpose()?
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))?;
    record
        .get(0)
        .map(ToString::to_string)
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))
}

fn parse_affected_rows(stdout: &str) -> Result<usize, DbOperationError> {
    last_csv_cell(stdout)
        .map_err(|error| match error {
            DbOperationError::EmptyResponse(_) => {
                DbOperationError::CommandTagParseFailed(stdout.to_string())
            }
            other => other,
        })?
        .parse::<usize>()
        .map_err(|error| DbOperationError::CommandTagParseFailed(error.to_string()))
}

fn parse_count_result(stdout: &str) -> Result<usize, DbOperationError> {
    first_csv_cell(stdout)
        .map_err(|error| match error {
            DbOperationError::EmptyResponse(_) => {
                DbOperationError::QueryFailed("Failed to parse COUNT result".to_string())
            }
            other => other,
        })?
        .parse::<usize>()
        .map_err(|error| {
            DbOperationError::QueryFailed(format!("Failed to parse COUNT result: {error}"))
        })
}

fn ddl_tag(query: &str) -> Option<CommandTag> {
    let object = second_keyword(query)
        .unwrap_or("OBJECT")
        .to_ascii_uppercase();
    let keyword = first_keyword(query);
    if keyword.eq_ignore_ascii_case("CREATE") {
        Some(CommandTag::Create(object))
    } else if keyword.eq_ignore_ascii_case("DROP") {
        Some(CommandTag::Drop(object))
    } else if keyword.eq_ignore_ascii_case("ALTER") {
        Some(CommandTag::Alter(object))
    } else {
        None
    }
}

fn transaction_control_tag(query: &str) -> Option<CommandTag> {
    match first_keyword(query).to_ascii_uppercase().as_str() {
        "BEGIN" => Some(CommandTag::Begin),
        "COMMIT" | "END" => Some(CommandTag::Commit),
        "ROLLBACK" if second_keyword(query).is_some_and(|kw| kw.eq_ignore_ascii_case("TO")) => {
            let name =
                if third_keyword(query).is_some_and(|kw| kw.eq_ignore_ascii_case("SAVEPOINT")) {
                    fourth_keyword(query)
                } else {
                    third_keyword(query)
                };
            Some(CommandTag::Other(format!(
                "ROLLBACK TO {}",
                name.unwrap_or("")
            )))
        }
        "ROLLBACK" => Some(CommandTag::Rollback),
        "SAVEPOINT" => Some(CommandTag::Other(format!(
            "SAVEPOINT {}",
            second_keyword(query).unwrap_or("")
        ))),
        "RELEASE" => Some(CommandTag::Other(format!(
            "RELEASE {}",
            second_keyword(query).unwrap_or("")
        ))),
        _ => None,
    }
}

fn dml_tag(query: &str, affected_rows: usize) -> Option<CommandTag> {
    let affected_rows = affected_rows as u64;
    match dml_keyword(query) {
        Some("INSERT") => Some(CommandTag::Insert(affected_rows)),
        Some("UPDATE") => Some(CommandTag::Update(affected_rows)),
        Some("DELETE") => Some(CommandTag::Delete(affected_rows)),
        _ => None,
    }
}

fn sqlite_side_effect_tag(query: &str) -> Option<CommandTag> {
    let keyword = first_keyword(query).to_ascii_uppercase();
    match keyword.as_str() {
        "ANALYZE" | "ATTACH" | "DETACH" | "REINDEX" | "VACUUM" => Some(CommandTag::Other(keyword)),
        _ => None,
    }
}

fn sqlite_statement_tags(statements: &[&str], changes: &HashMap<usize, usize>) -> Vec<CommandTag> {
    statements
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            dml_tag(statement, *changes.get(&index).unwrap_or(&0))
                .or_else(|| ddl_tag(statement))
                .or_else(|| transaction_control_tag(statement))
                .or_else(|| sqlite_side_effect_tag(statement))
        })
        .collect()
}

fn discard_rolled_back(tags: &[CommandTag]) -> Vec<CommandTag> {
    let mut effective = Vec::new();
    let mut frames: Vec<(Option<String>, Vec<CommandTag>)> = Vec::new();

    for tag in tags {
        match tag {
            CommandTag::Begin => frames.push((None, Vec::new())),
            CommandTag::Other(raw) if raw == "SAVEPOINT" || raw.starts_with("SAVEPOINT ") => {
                frames.push((tag_name(raw, "SAVEPOINT"), Vec::new()));
            }
            CommandTag::Other(raw) if raw == "RELEASE" || raw.starts_with("RELEASE ") => {
                if let Some(index) = savepoint_frame_index(&frames, tag_name(raw, "RELEASE"))
                    && index > 0
                {
                    let mut merged = Vec::new();
                    for (_, frame) in frames.drain(index..) {
                        merged.extend(frame);
                    }
                    if let Some((_, parent)) = frames.last_mut() {
                        parent.extend(merged);
                    }
                }
            }
            CommandTag::Other(raw) if raw == "ROLLBACK TO" || raw.starts_with("ROLLBACK TO ") => {
                if let Some(index) = savepoint_frame_index(&frames, tag_name(raw, "ROLLBACK TO")) {
                    frames.truncate(index + 1);
                    if let Some((_, frame)) = frames.last_mut() {
                        frame.clear();
                    }
                }
            }
            CommandTag::Rollback => {
                frames.clear();
            }
            CommandTag::Commit => {
                for (_, frame) in frames.drain(..) {
                    effective.extend(frame);
                }
            }
            _ => {
                if let Some((_, frame)) = frames.last_mut() {
                    frame.push(tag.clone());
                } else {
                    effective.push(tag.clone());
                }
            }
        }
    }

    for (_, frame) in frames.drain(..) {
        effective.extend(frame);
    }

    effective
}

fn tag_name(raw: &str, prefix: &str) -> Option<String> {
    raw.strip_prefix(prefix)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_uppercase)
}

fn savepoint_frame_index(
    frames: &[(Option<String>, Vec<CommandTag>)],
    name: Option<String>,
) -> Option<usize> {
    frames
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, (frame_name, _))| {
            if index == 0 {
                None
            } else if name
                .as_ref()
                .is_none_or(|name| frame_name.as_ref() == Some(name))
            {
                Some(index)
            } else {
                None
            }
        })
}

fn aggregate_sqlite_command_tag(tags: &[CommandTag]) -> Option<CommandTag> {
    let effective = discard_rolled_back(tags);
    if let Some(tag) = effective.iter().find(|tag| tag.is_schema_modifying()) {
        return Some(tag.clone());
    }
    if let Some(tag) = effective.iter().rev().find(|tag| tag.needs_refresh()) {
        return Some(tag.clone());
    }
    if tags.iter().any(CommandTag::needs_refresh) {
        return Some(CommandTag::Rollback);
    }
    tags.last().cloned()
}

fn command_tag_result(
    query: &str,
    tag: CommandTag,
    elapsed: u64,
    source: QuerySource,
) -> QueryResult {
    QueryResult::success(query.to_string(), Vec::new(), Vec::new(), elapsed, source)
        .with_row_count(tag.affected_rows().unwrap_or(0) as usize)
        .with_command_tag(tag)
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
        Self::validate_main_schema(schema)?;
        self.table_detail_with_mode(Self::path_from_dsn(dsn)?, table, true)
            .await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        Self::validate_main_schema(schema)?;
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

#[async_trait]
impl QueryExecutor for SqliteAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        Self::validate_main_schema(schema)?;
        let path = Self::path_from_dsn(dsn)?;
        let order_columns = self.preview_order_columns(path, table).await;
        let columns = self
            .columns(path, table)
            .await
            .map(Self::preview_visible_column_names)
            .unwrap_or_default();
        let query = sql::build_preview_query(table, &columns, &order_columns, limit, offset);
        self.execute_quoted_query(path, &query, QuerySource::Preview, read_only)
            .await
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        let path = Self::path_from_dsn(dsn)?;
        let marker = sqlite_probe_marker();
        let execution_query = sqlite_adhoc_execution_query(query, &marker);
        let statements = split_sqlite_statements(query);

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self
            .cli
            .execute_quote(path, &execution_query, read_only)
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let (stdout, changes) = strip_sqlite_probes(&stdout, &marker)?;
        let stdout = last_sqlite_result_set(&stdout, &marker)?.unwrap_or(stdout);
        let tag = aggregate_sqlite_command_tag(&sqlite_statement_tags(&statements, &changes));

        if stdout.trim().is_empty() {
            if let Some(tag) = tag {
                return Ok(command_tag_result(query, tag, elapsed, QuerySource::Adhoc));
            }
            let mut result = QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                QuerySource::Adhoc,
            );
            if statements
                .iter()
                .any(|stmt| statement_counts_as_select_tag(stmt))
            {
                result = result.with_command_tag(CommandTag::Select(0));
            }
            return Ok(result);
        }

        let mut result = quoted_to_query_result(query, &stdout, QuerySource::Adhoc, elapsed)?;
        if let Some(tag) = tag {
            result = result.with_command_tag(tag);
        } else if statements
            .iter()
            .any(|stmt| statement_counts_as_select_tag(stmt))
        {
            let row_count = result.row_count() as u64;
            result = result.with_command_tag(CommandTag::Select(row_count));
        }
        Ok(result)
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        let (affected_rows, execution_time_ms) = self
            .execute_changes_query(Self::path_from_dsn(dsn)?, query, read_only)
            .await?;
        Ok(WriteExecutionResult {
            affected_rows,
            execution_time_ms,
        })
    }

    async fn count_query_rows(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        let stdout = self
            .cli
            .execute_csv(Self::path_from_dsn(dsn)?, query, read_only)
            .await?;
        parse_count_result(&stdout)
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        path: &std::path::Path,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        if !is_sqlite_rerunnable_export_query(query) {
            return Err(sqlite_export_not_rerunnable_error());
        }
        self.cli
            .export_csv(Self::path_from_dsn(dsn)?, query, path, read_only)
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support::make_sqlite_db;
    use crate::app::ports::outbound::{DdlGenerator, MetadataProvider, QueryExecutor, SqlDialect};
    use crate::domain::{CommandTag, DatabaseType, QuerySource};

    use super::*;

    #[test]
    fn split_sqlite_statements_ignores_semicolons_in_literals_and_comments() {
        let statements = split_sqlite_statements(
            "INSERT INTO logs(message) VALUES ('a;b'); -- ; ignored\nSELECT ';' AS value;",
        );

        assert_eq!(
            statements,
            vec![
                "INSERT INTO logs(message) VALUES ('a;b')",
                "-- ; ignored\nSELECT ';' AS value"
            ]
        );
    }

    #[test]
    fn append_changes_wraps_multi_statement_write_without_explicit_transaction() {
        let query = "INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2);";

        let wrapped = append_changes_query(query);

        assert_eq!(
            wrapped,
            "BEGIN;\nINSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn append_changes_wraps_multi_statement_replace_without_explicit_transaction() {
        let query = "REPLACE INTO users(id) VALUES (1); SELECT * FROM missing";

        let wrapped = append_changes_query(query);

        assert_eq!(
            wrapped,
            "BEGIN;\nREPLACE INTO users(id) VALUES (1); SELECT * FROM missing\n;\nCOMMIT\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn append_changes_keeps_explicit_begin_commit_transaction_control() {
        let query = "BEGIN; INSERT INTO users(id) VALUES (1); COMMIT";

        let wrapped = append_changes_query(query);

        assert_eq!(
            wrapped,
            "BEGIN; INSERT INTO users(id) VALUES (1); COMMIT\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn append_changes_keeps_explicit_begin_end_transaction_control() {
        let query = "BEGIN; INSERT INTO users(id) VALUES (1); END";

        let wrapped = append_changes_query(query);

        assert_eq!(
            wrapped,
            "BEGIN; INSERT INTO users(id) VALUES (1); END\n;\nSELECT changes() AS affected_rows;"
        );
    }

    #[test]
    fn sqlite_side_effect_statements_emit_refresh_tags() {
        let changes = HashMap::new();

        let tags = sqlite_statement_tags(
            &[
                "ANALYZE users",
                "ATTACH DATABASE 'other.db' AS other",
                "DETACH DATABASE other",
                "REINDEX users_name_idx",
                "VACUUM",
            ],
            &changes,
        );

        assert_eq!(
            tags,
            vec![
                CommandTag::Other("ANALYZE".to_string()),
                CommandTag::Other("ATTACH".to_string()),
                CommandTag::Other("DETACH".to_string()),
                CommandTag::Other("REINDEX".to_string()),
                CommandTag::Other("VACUUM".to_string()),
            ]
        );
        assert!(tags.iter().all(CommandTag::needs_refresh));
    }

    #[test]
    fn index_key_column_names_preserves_expression_and_unknown_key_columns() {
        let columns = vec![
            RawIndexColumn {
                seqno: 0,
                cid: 1,
                name: Some("email".to_string()),
                key: 1,
            },
            RawIndexColumn {
                seqno: 1,
                cid: -2,
                name: None,
                key: 1,
            },
            RawIndexColumn {
                seqno: 2,
                cid: 99,
                name: None,
                key: 1,
            },
            RawIndexColumn {
                seqno: 3,
                cid: 2,
                name: Some("rowid".to_string()),
                key: 0,
            },
        ];

        assert_eq!(
            SqliteAdapter::index_key_column_names(&columns),
            vec![
                "email".to_string(),
                "<expression>".to_string(),
                "<unknown>".to_string()
            ]
        );
    }

    mod preview {
        use super::*;

        #[tokio::test]
        async fn returns_columns_rows_and_respects_pagination() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (2, 'b'), (1, 'a'), (3, 'c');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 1, 1, true)
                .await
                .unwrap();

            assert_eq!(result.source, QuerySource::Preview);
            assert_eq!(result.columns, vec!["id", "name"]);
            assert_eq!(result.rows(), vec![vec!["2".to_string(), "b".to_string()]]);
        }

        #[tokio::test]
        async fn rejects_non_main_schema() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "other", "users", 10, 0, true)
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        }

        #[tokio::test]
        async fn preserves_nul_text_primary_key_for_preview_and_delete() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id TEXT PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES ('a' || char(0) || 'bc', 'target'), ('only', 'other');
            ",
            );
            let adapter = SqliteAdapter::new();

            let preview = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(
                preview.value_at(0, 0),
                Some(&QueryValue::Text("a\0bc".to_string()))
            );
            assert_eq!(preview.rows()[0][0], "a\\0bc");

            let delete_sql = adapter.build_bulk_delete_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                &[vec![(
                    "id".to_string(),
                    QueryValue::Text("a\0bc".to_string()),
                )]],
            );
            let write = adapter
                .execute_write(&dsn, &delete_sql, false)
                .await
                .unwrap();
            assert_eq!(write.affected_rows, 1);

            let remaining = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
                .await
                .unwrap();
            assert_eq!(remaining.row_count(), 1);
            assert_eq!(
                remaining.value_at(0, 0),
                Some(&QueryValue::Text("only".to_string()))
            );
        }

        #[tokio::test]
        async fn excludes_hidden_columns_from_preview_select_list() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            INSERT INTO notes_fts(body) VALUES ('hello');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "notes_fts", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["body"]);
            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::Text("hello".to_string()))
            );
        }

        #[tokio::test]
        async fn preserves_distinct_c0_text_values_in_preview() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, value TEXT);
            INSERT INTO users(value) VALUES (char(1) || char(1));
            INSERT INTO users(value) VALUES (char(1) || char(92) || 'u0001');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::Text("\x01\x01".to_string()))
            );
            assert_eq!(
                result.value_at(1, 1),
                Some(&QueryValue::Text("\x01\\u0001".to_string()))
            );
        }

        #[tokio::test]
        async fn preserves_sentinel_like_text_without_nul_in_preview() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, token TEXT);
            INSERT INTO users(token) VALUES (char(1) || 'SABIQL_HEX:4142');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::Text(format!(
                    "{}4142",
                    sql::sqlite_nul_text_sentinel()
                )))
            );
        }

        #[tokio::test]
        async fn keeps_generated_columns_in_preview_select_list() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                name TEXT,
                name_upper TEXT GENERATED ALWAYS AS (upper(name)) STORED
            );
            INSERT INTO users(name) VALUES ('alice');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["id", "name", "name_upper"]);
            assert_eq!(
                result.value_at(0, 2),
                Some(&QueryValue::Text("ALICE".to_string()))
            );
        }
    }

    mod parsing {
        use super::*;

        #[test]
        fn quoted_to_query_result_preserves_newline_for_single_statement() {
            let quoted = "'body','marker'\n'line 1\nline 2','ok'\n";

            let result = quoted_to_query_result(
                "SELECT body, marker FROM notes",
                quoted,
                QuerySource::Adhoc,
                1,
            )
            .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
        }

        #[test]
        fn last_sqlite_result_set_uses_marker_boundaries() {
            let marker = "probe";
            let sqlite_output_with_ignored_first_result_set = "'ignored'\n1\n'probe_result_stmt','probe_result_marker'\n0,'probe'\n'body','marker'\n'line 1\nline 2','ok'\n'probe_result_stmt','probe_result_marker'\n1,'probe'\n";

            let quoted =
                last_sqlite_result_set(sqlite_output_with_ignored_first_result_set, marker)
                    .unwrap()
                    .unwrap();
            let result = quoted_to_query_result(
                "SELECT 1 AS ignored; SELECT body, marker FROM notes",
                &quoted,
                QuerySource::Adhoc,
                1,
            )
            .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
        }

        #[test]
        fn quoted_to_query_result_preserves_sqlite_value_kinds() {
            let quoted = "'a','b','c','d'\nNULL,'','NULL',X'00FF41'\n";

            let result =
                quoted_to_query_result("SELECT a, b, c, d FROM t", quoted, QuerySource::Adhoc, 1)
                    .unwrap();

            assert_eq!(
                result.rows(),
                vec![vec![
                    "NULL".to_string(),
                    String::new(),
                    "NULL".to_string(),
                    "BLOB (3 bytes) 00 FF 41".to_string()
                ]]
            );
            assert!(matches!(result.value_at(0, 0), Some(QueryValue::Null)));
            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::Text(String::new()))
            );
            assert_eq!(
                result.value_at(0, 2),
                Some(&QueryValue::Text("NULL".to_string()))
            );
            assert_eq!(
                result.value_at(0, 3),
                Some(&QueryValue::Blob(vec![0, 255, 65]))
            );
        }

        #[test]
        fn quoted_to_query_result_normalizes_infinite_numeric_literals() {
            let quoted = "'pos','neg'\nInf,-Inf\n";

            let result =
                quoted_to_query_result("SELECT 1e999, -1e999", quoted, QuerySource::Adhoc, 1)
                    .unwrap();

            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::SqlLiteral("1e999".to_string()))
            );
            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::SqlLiteral("-1e999".to_string()))
            );
        }

        #[test]
        fn quoted_to_query_result_rejects_unterminated_quote() {
            let result = quoted_to_query_result(
                "SELECT body FROM notes",
                "'body'\n'unclosed\nnext",
                QuerySource::Adhoc,
                1,
            );

            assert!(matches!(
                result,
                Err(DbOperationError::MetadataParseFailed(message))
                    if message == "unterminated SQLite quoted output"
            ));
        }

        #[test]
        fn parse_unistr_inner_sql_escapes_does_not_decode_unicode_sequences() {
            assert_eq!(
                parse_unistr_inner_sql_escapes("unistr('\\u0001\\u0001')").unwrap(),
                "\\u0001\\u0001"
            );
            assert_eq!(
                parse_unistr_inner_sql_escapes("unistr('\\u0001O''Reilly')").unwrap(),
                "\\u0001O'Reilly"
            );
        }

        #[test]
        fn decode_preview_transport_unistr_decodes_hex_payload() {
            assert_eq!(
                decode_preview_transport_unistr("unistr('\\u0001SABIQL_HEX:61006263')")
                    .unwrap()
                    .as_deref(),
                Some("a\0bc")
            );
            assert_eq!(
                decode_preview_transport_unistr("unistr('\\u0001SABIQL_HEX:0101')")
                    .unwrap()
                    .as_deref(),
                Some("\x01\x01")
            );
            assert_eq!(
                decode_preview_transport_unistr("unistr('\\u0001SABIQL_HEX:015C7530303031')")
                    .unwrap()
                    .as_deref(),
                Some("\x01\\u0001")
            );
        }

        #[test]
        fn parse_quoted_value_keeps_unrecoverable_adhoc_unistr_as_sql_literal() {
            assert_eq!(
                parse_quoted_value("unistr('\\u0001\\u0001')", QuerySource::Adhoc, true).unwrap(),
                QueryValue::SqlLiteral("unistr('\\u0001\\u0001')".to_string())
            );
            assert_eq!(
                parse_quoted_value("unistr('\\u0001O''Reilly')", QuerySource::Adhoc, true).unwrap(),
                QueryValue::SqlLiteral("unistr('\\u0001O''Reilly')".to_string())
            );
        }

        #[test]
        fn parse_quoted_value_decodes_preview_transport_unistr() {
            let value = parse_quoted_value(
                "unistr('\\u0001SABIQL_HEX:61006263')",
                QuerySource::Preview,
                true,
            )
            .unwrap();

            assert_eq!(value, QueryValue::Text("a\0bc".to_string()));
        }

        #[test]
        fn parse_quoted_value_decodes_preview_transport_plain_quoted() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let quoted = format!("'{sentinel}68656C6C6F'");
            let value = parse_quoted_value(&quoted, QuerySource::Preview, true).unwrap();

            assert_eq!(value, QueryValue::Text("hello".to_string()));
        }

        #[test]
        fn parse_quoted_value_keeps_plain_quoted_transport_as_text_for_adhoc() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let transport = format!("{sentinel}68656C6C6F");
            let quoted = format!("'{transport}'");
            let value = parse_quoted_value(&quoted, QuerySource::Adhoc, true).unwrap();

            assert_eq!(value, QueryValue::Text(transport));
        }

        #[test]
        fn parse_quoted_value_skips_preview_transport_decode_for_column_names() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let transport = format!("{sentinel}68656C6C6F");
            let quoted = format!("'{transport}'");
            let value = parse_quoted_value(&quoted, QuerySource::Preview, false).unwrap();

            assert_eq!(value, QueryValue::Text(transport));
        }

        #[test]
        fn quoted_to_query_result_keeps_transport_like_column_name() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let column = format!("{sentinel}4142");
            let data = format!("'{sentinel}68656C6C6F'");
            let quoted = format!("'{column}'\n{data}\n");
            let result =
                quoted_to_query_result("SELECT 1", &quoted, QuerySource::Preview, 1).unwrap();

            assert_eq!(result.columns, vec![column]);
            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::Text("hello".to_string()))
            );
        }

        #[test]
        fn quoted_to_query_result_keeps_unrecoverable_adhoc_unistr_as_sql_literal() {
            let quoted = "'value'\nunistr('\\u0001\\u0001')\n";

            let result =
                quoted_to_query_result("SELECT char(1) || char(1)", quoted, QuerySource::Adhoc, 1)
                    .unwrap();

            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::SqlLiteral(
                    "unistr('\\u0001\\u0001')".to_string()
                ))
            );
        }

        #[test]
        fn parse_affected_rows_reads_trailing_changes_cell() {
            assert_eq!(parse_affected_rows("changes()\n3\n").unwrap(), 3);
        }

        #[test]
        fn strip_sqlite_probes_removes_probe_result_sets() {
            let marker = "probe";
            let stdout = "'id','name'\n1,'Alice'\n'probe_stmt','probe_changes'\n0,2\n'value'\n42\n";

            let (filtered, changes) = strip_sqlite_probes(stdout, marker).unwrap();

            assert_eq!(changes.get(&0), Some(&2));
            assert_eq!(filtered, "'id','name'\n1,'Alice'\n'value'\n42");
        }

        #[test]
        fn parse_count_result_reads_first_result_cell() {
            assert_eq!(parse_count_result("COUNT(*)\n42\n").unwrap(), 42);
        }
    }

    mod adhoc_execution {
        use super::*;

        #[tokio::test]
        async fn select_returns_query_result() {
            let (_dir, dsn) =
                make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS value", true)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["value"]);
            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn select_preserves_quoted_newline_in_multicolumn_result() {
            let (_dir, dsn) =
                make_sqlite_db("CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT 'line 1' || char(10) || 'line 2' AS body, 'ok' AS marker",
                    true,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn multi_select_preserves_quoted_newline_in_last_result() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT 1 AS ignored; SELECT 'line 1' || char(10) || 'line 2' AS body, 'ok' AS marker",
                true,
            )
            .await
            .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn multi_select_does_not_treat_data_row_as_next_header() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT 1 AS a, 2 AS b UNION ALL SELECT 3, 4; SELECT 5 AS c, 6 AS d",
                    true,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["c", "d"]);
            assert_eq!(result.rows(), vec![vec!["5".to_string(), "6".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn multi_select_empty_trailing_result_returns_empty_result() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS a; SELECT 2 AS b WHERE false", true)
                .await
                .unwrap();

            assert!(result.columns.is_empty());
            assert!(result.rows().is_empty());
            assert_eq!(result.command_tag, Some(CommandTag::Select(0)));
        }

        #[tokio::test]
        async fn pragma_result_does_not_get_select_command_tag() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA table_info(users)", true)
                .await
                .unwrap();

            assert_eq!(
                result.columns,
                vec!["cid", "name", "type", "notnull", "dflt_value", "pk"]
            );
            assert_eq!(result.command_tag, None);
        }

        #[tokio::test]
        async fn enables_foreign_keys_before_user_sql() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA foreign_keys", false)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn read_only_session_enables_query_only_before_user_sql() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA query_only", true)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn applies_busy_timeout_before_user_sql() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA busy_timeout", true)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec![cli::BUSY_TIMEOUT_MS.to_string()]]);
        }

        #[tokio::test]
        async fn values_result_does_not_get_select_command_tag() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "VALUES (1)", true)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
            assert_eq!(result.command_tag, None);
        }

        #[tokio::test]
        async fn dml_returns_affected_rows_command_tag() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "UPDATE users SET name = 'x' WHERE id = 1", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
        }

        #[tokio::test]
        async fn replace_into_returns_insert_refresh_tag() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "REPLACE INTO users(id, name) VALUES (1, 'z')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn dml_with_following_select_uses_trailing_changes_result() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "UPDATE users SET name = 'x' WHERE id = 1; SELECT 42",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
        }

        #[tokio::test]
        async fn dml_with_following_select_preserves_result_set_and_refresh_tag() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "UPDATE users SET name = 'x' WHERE id = 1; SELECT name FROM users",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["name"]);
            assert_eq!(result.rows(), vec![vec!["x".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
        }

        #[tokio::test]
        async fn multi_dml_uses_last_effective_refresh_tag() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "INSERT INTO users(id, name) VALUES (2, 'b'), (3, 'c');
                     UPDATE users SET name = 'z' WHERE id IN (1, 2);
                     DELETE FROM users WHERE id = 3",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
        }

        #[tokio::test]
        async fn ddl_wins_over_later_dml_for_refresh_tag() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "CREATE TABLE users(id INTEGER PRIMARY KEY);
                     INSERT INTO users(id) VALUES (1)",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(
                result.command_tag,
                Some(CommandTag::Create("TABLE".to_string()))
            );
            assert_eq!(result.row_count(), 0);
        }

        #[tokio::test]
        async fn rolled_back_dml_returns_rollback_tag() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN; INSERT INTO users(id) VALUES (1); ROLLBACK",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn full_rollback_inside_savepoint_discards_outer_dml() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn savepoint_rollback_discards_inner_dml_only() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     COMMIT",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn rollback_to_keeps_savepoint_for_later_rollback() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     INSERT INTO users(id) VALUES (3);
                     ROLLBACK TO sp;
                     COMMIT",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn rollback_to_named_outer_savepoint_discards_nested_frames() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT outer_sp;
                     INSERT INTO users(id) VALUES (2);
                     SAVEPOINT inner_sp;
                     INSERT INTO users(id) VALUES (3);
                     ROLLBACK TO outer_sp;
                     COMMIT",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn with_insert_reports_affected_rows_command_tag() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "WITH payload(id) AS (VALUES (1), (2))
                     INSERT INTO users(id) SELECT id FROM payload",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 2);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(2)));
        }

        #[tokio::test]
        async fn multi_statement_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)",
                    false,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn returning_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();
            let query =
                "INSERT INTO users(id) VALUES (1) RETURNING id; INSERT INTO missing(id) VALUES (2)";

            let result = adapter.execute_adhoc(&dsn, query, false).await;

            assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn select_then_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT 1 AS marker; INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)",
                    false,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn dml_with_trailing_line_comment_returns_affected_rows() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "DELETE FROM users WHERE id = 1 -- cleanup selected row",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
        }

        #[tokio::test]
        async fn dml_returning_preserves_returned_rows() {
            let (_dir, dsn) =
                make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "INSERT INTO users(name) VALUES ('a') RETURNING id, name",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["id", "name"]);
            assert_eq!(result.rows(), vec![vec!["1".to_string(), "a".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn update_returning_preserves_returned_rows() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "UPDATE users SET name = 'x' RETURNING id, name",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.rows().len(), 2);
            assert_eq!(result.command_tag, Some(CommandTag::Update(2)));
        }

        #[tokio::test]
        async fn delete_returning_preserves_returned_rows() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "DELETE FROM users WHERE id = 1 RETURNING id, name",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string(), "a".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
        }

        #[tokio::test]
        async fn dml_table_name_containing_returning_reports_affected_rows() {
            let (_dir, dsn) =
                make_sqlite_db("CREATE TABLE returning_log(id INTEGER PRIMARY KEY, name TEXT);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "INSERT INTO returning_log(name) VALUES ('a')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn dml_backtick_quoted_identifier_containing_returning_reports_affected_rows() {
            let (_dir, dsn) =
                make_sqlite_db("CREATE TABLE `my returning`(id INTEGER PRIMARY KEY, name TEXT);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "INSERT INTO `my returning`(name) VALUES ('a')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn dml_bracket_quoted_identifier_containing_returning_reports_affected_rows() {
            let (_dir, dsn) =
                make_sqlite_db("CREATE TABLE [my returning](id INTEGER PRIMARY KEY, name TEXT);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "INSERT INTO [my returning](name) VALUES ('a')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn ddl_returns_schema_refresh_command_tag() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "CREATE TABLE users(id INTEGER PRIMARY KEY)", false)
                .await
                .unwrap();

            assert_eq!(
                result.command_tag,
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }
    }

    mod write_execution {
        use super::*;

        #[tokio::test]
        async fn foreign_key_restrict_rejects_parent_delete_with_child_row() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                org_id INTEGER REFERENCES orgs(id) ON DELETE RESTRICT
            );
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, org_id) VALUES (1, 1);
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", false)
                .await;
            let children = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert!(
                matches!(result, Err(DbOperationError::QueryFailed(message)) if message.contains("FOREIGN KEY constraint failed"))
            );
            assert_eq!(children.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn foreign_key_cascade_applies_to_parent_delete() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                org_id INTEGER REFERENCES orgs(id) ON DELETE CASCADE
            );
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, org_id) VALUES (1, 1);
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", false)
                .await
                .unwrap();
            let children = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.affected_rows, 1);
            assert!(children.rows().is_empty());
        }

        #[tokio::test]
        async fn returns_affected_rows() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "DELETE FROM users WHERE id IN (1, 2)", false)
                .await
                .unwrap();

            assert_eq!(result.affected_rows, 2);
        }

        #[tokio::test]
        async fn count_query_rows_parses_count_result() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            INSERT INTO users(id) VALUES (1), (2), (3);
            ",
            );
            let adapter = SqliteAdapter::new();

            let count = adapter
                .count_query_rows(&dsn, "SELECT COUNT(*) FROM users", true)
                .await
                .unwrap();

            assert_eq!(count, 3);
        }

        #[tokio::test]
        async fn export_to_csv_writes_rows_and_returns_row_count() {
            let (dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let path = dir.path().join("users.csv");
            let adapter = SqliteAdapter::new();

            let row_count = adapter
                .export_to_csv(&dsn, "SELECT id, name FROM users ORDER BY id", &path, true)
                .await
                .unwrap();
            let csv = std::fs::read_to_string(path).unwrap();

            assert_eq!(row_count, 2);
            assert_eq!(csv, "id,name\n1,a\n2,b\n");
        }

        #[tokio::test]
        async fn export_to_csv_rejects_write_sql() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let path = std::env::temp_dir().join("sabiql_write_export.csv");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .export_to_csv(&dsn, "INSERT INTO users(id) VALUES (1)", &path, false)
                .await;

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(message))
                if message.contains("write or DDL")
            ));
            assert!(!path.exists());
        }

        #[tokio::test]
        async fn read_only_write_fails() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "INSERT INTO users(id) VALUES (1)", true)
                .await;

            assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
        }
    }

    mod metadata {
        use super::*;

        #[tokio::test]
        async fn lists_user_tables_in_main_schema() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY AUTOINCREMENT);
            ",
            );
            let adapter = SqliteAdapter::new();

            let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

            assert_eq!(metadata.schemas, vec![Schema::new("main")]);
            assert_eq!(metadata.table_summaries.len(), 1);
            assert_eq!(metadata.table_summaries[0].qualified_name(), "main.users");
            assert_eq!(metadata.table_summaries[0].row_count_estimate, Some(0));
        }

        #[tokio::test]
        async fn empty_database_returns_no_tables() {
            let (_dir, dsn) = make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

            assert_eq!(metadata.schemas, vec![Schema::new("main")]);
            assert!(metadata.table_summaries.is_empty());
        }

        #[tokio::test]
        async fn hides_fts5_shadow_tables_from_normal_table_list() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT);
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            ",
            );
            let adapter = SqliteAdapter::new();

            let metadata = adapter.fetch_metadata(&dsn).await.unwrap();
            let table_names: Vec<_> = metadata
                .table_summaries
                .iter()
                .map(|summary| summary.name.as_str())
                .collect();

            assert_eq!(table_names, vec!["notes", "notes_fts"]);
        }

        #[tokio::test]
        async fn hides_rtree_shadow_tables_from_normal_table_list() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE places(id INTEGER PRIMARY KEY, name TEXT);
            CREATE VIRTUAL TABLE places_geo USING rtree(
                id,
                minX, maxX,
                minY, maxY
            );
            ",
            );
            let adapter = SqliteAdapter::new();

            let metadata = adapter.fetch_metadata(&dsn).await.unwrap();
            let table_names: Vec<_> = metadata
                .table_summaries
                .iter()
                .map(|summary| summary.name.as_str())
                .collect();

            assert_eq!(table_names, vec!["places", "places_geo"]);
        }

        #[tokio::test]
        async fn detects_virtual_tables_in_schema() {
            let (_dir, dsn) = make_sqlite_db("CREATE VIRTUAL TABLE notes_fts USING fts5(body);");
            let adapter = SqliteAdapter::new();
            let path = SqliteAdapter::path_from_dsn(&dsn).unwrap();

            assert!(adapter.has_virtual_tables(path).await.unwrap());
        }

        #[tokio::test]
        async fn simple_schema_has_no_virtual_tables() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();
            let path = SqliteAdapter::path_from_dsn(&dsn).unwrap();

            assert!(!adapter.has_virtual_tables(path).await.unwrap());
        }
    }

    mod table_detail {
        use super::*;

        #[tokio::test]
        async fn loads_columns_indexes_and_foreign_keys() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                org_id INTEGER REFERENCES orgs(id) ON DELETE CASCADE
            );
            CREATE INDEX idx_users_org_id ON users(org_id);
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, email, org_id) VALUES (1, 'a@example.com', 1);
            ",
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
                        && index.columns == vec!["org_id".to_string()]
                        && index.index_type == IndexType::Unknown)
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
        async fn without_primary_key_sets_primary_key_none() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE logs(message TEXT);");
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
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(email TEXT UNIQUE NOT NULL);");
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
        async fn partial_unique_index_does_not_mark_column_unique() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(email TEXT);
            CREATE UNIQUE INDEX idx_users_email_active
                ON users(email)
                WHERE email IS NOT NULL;
            ",
            );
            let adapter = SqliteAdapter::new();

            let detail = adapter
                .fetch_table_detail(&dsn, "main", "users")
                .await
                .unwrap();
            let email = detail
                .columns
                .iter()
                .find(|column| column.name == "email")
                .unwrap();
            assert!(!email.is_unique());
            let index = detail
                .indexes
                .iter()
                .find(|index| index.name == "idx_users_email_active")
                .unwrap();
            assert!(index.is_unique());
            assert!(index.is_partial());
            assert_eq!(index.columns, vec!["email".to_string()]);

            let light = adapter
                .fetch_table_columns_and_fks(&dsn, "main", "users")
                .await
                .unwrap();
            let light_email = light
                .columns
                .iter()
                .find(|column| column.name == "email")
                .unwrap();
            assert!(!light_email.is_unique());
            assert!(light.indexes.is_empty());
        }

        #[tokio::test]
        async fn generated_and_hidden_columns_are_read_only() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                name TEXT,
                name_upper TEXT GENERATED ALWAYS AS (upper(name)) STORED
            );
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            ",
            );
            let adapter = SqliteAdapter::new();

            let users = adapter
                .fetch_table_detail(&dsn, "main", "users")
                .await
                .unwrap();
            let generated = users
                .columns
                .iter()
                .find(|column| column.name == "name_upper")
                .unwrap();
            assert!(generated.is_read_only());
            assert!(generated.is_generated());
            assert_eq!(generated.read_only_reason(), Some("generated"));

            let fts = adapter
                .fetch_table_detail(&dsn, "main", "notes_fts")
                .await
                .unwrap();
            let hidden = fts
                .columns
                .iter()
                .find(|column| column.name == "notes_fts")
                .unwrap();
            assert!(hidden.is_read_only());
            assert!(hidden.is_hidden());
            assert_eq!(hidden.read_only_reason(), Some("hidden"));
        }

        #[tokio::test]
        async fn source_ddl_preserves_without_rowid_and_virtual_table_syntax() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE settings(
                key TEXT PRIMARY KEY,
                value TEXT
            ) WITHOUT ROWID;
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            ",
            );
            let adapter = SqliteAdapter::new();

            let without_rowid = adapter
                .fetch_table_detail(&dsn, "main", "settings")
                .await
                .unwrap();
            assert!(
                without_rowid
                    .source_ddl()
                    .is_some_and(|ddl| ddl.contains("WITHOUT ROWID"))
            );
            assert_eq!(
                adapter.generate_ddl(DatabaseType::SQLite, &without_rowid),
                without_rowid.source_ddl().unwrap()
            );

            let virtual_table = adapter
                .fetch_table_detail(&dsn, "main", "notes_fts")
                .await
                .unwrap();
            assert!(
                virtual_table
                    .source_ddl()
                    .is_some_and(|ddl| ddl.starts_with("CREATE VIRTUAL TABLE"))
            );
        }

        #[tokio::test]
        async fn partial_expression_index_preserves_metadata_and_definition() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT);
            CREATE INDEX idx_users_email_lower
                ON users(lower(email))
                WHERE email IS NOT NULL;
            ",
            );
            let adapter = SqliteAdapter::new();

            let detail = adapter
                .fetch_table_detail(&dsn, "main", "users")
                .await
                .unwrap();
            let index = detail
                .indexes
                .iter()
                .find(|index| index.name == "idx_users_email_lower")
                .unwrap();

            assert_eq!(index.columns, vec!["<expression>".to_string()]);
            assert!(index.is_partial());
            assert!(index.has_expression());
            assert!(index.has_auxiliary_columns());
            assert!(index.definition.as_deref().is_some_and(|definition| {
                definition.contains("lower(email)")
                    && definition.contains("WHERE email IS NOT NULL")
            }));
        }
    }

    mod foreign_keys {
        use super::*;

        #[tokio::test]
        async fn composite_foreign_key_groups_columns_in_sequence_order() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE parent(a INTEGER, b INTEGER, PRIMARY KEY(a, b));
            CREATE TABLE child(
                x INTEGER,
                y INTEGER,
                FOREIGN KEY(x, y) REFERENCES parent(a, b)
            );
            ",
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
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE parent(a INTEGER, b INTEGER, PRIMARY KEY(a, b));
            CREATE TABLE child(
                x INTEGER,
                y INTEGER,
                FOREIGN KEY(x, y) REFERENCES parent
            );
            ",
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
    }

    mod dsn_validation {
        use super::*;

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
        async fn relative_path_starting_with_dash_is_opened_as_database_path() {
            struct CleanupPath(String);
            impl Drop for CleanupPath {
                fn drop(&mut self) {
                    let _ = std::fs::remove_file(&self.0);
                }
            }

            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = format!("-sabiql-{unique}.db");
            let _cleanup = CleanupPath(path.clone());
            let dsn = format!("sqlite://{path}");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS value", false)
                .await;

            let result = result.unwrap();
            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
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
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter.fetch_table_detail(&dsn, "other", "users").await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        }

        #[tokio::test]
        async fn missing_table_returns_object_missing() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter.fetch_table_detail(&dsn, "main", "missing").await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        }
    }

    mod table_signatures {
        use super::*;

        #[tokio::test]
        async fn change_with_table_shape() {
            let (_dir, dsn) = make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();

            assert_eq!(signatures.len(), 1);
            assert_eq!(signatures[0].qualified_name(), "main.users");
            assert!(signatures[0].signature.contains("CREATE TABLE users"));
            assert!(signatures[0].signature.contains("col=id:INTEGER"));
        }

        #[tokio::test]
        async fn include_foreign_key_update_action() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                org_id INTEGER REFERENCES orgs(id)
                    ON DELETE CASCADE
                    ON UPDATE SET NULL
            );
            ",
            );
            let adapter = SqliteAdapter::new();

            let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();
            let signature = signatures
                .iter()
                .find(|signature| signature.name == "users")
                .unwrap();

            assert!(
                signature
                    .signature
                    .contains("fk=fk_users_0:org_id:orgs:id:CASCADE:SET NULL")
            );
        }
    }
}
