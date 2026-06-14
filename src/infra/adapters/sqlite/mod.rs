use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider, QueryExecutor};
use crate::domain::{
    Column, ColumnAttributes, CommandTag, DatabaseMetadata, FkAction, ForeignKey, Index,
    IndexAttributes, IndexType, QueryResult, QuerySource, Schema, Table, TableSignature,
    TableSummary, WriteExecutionResult,
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
}

#[derive(Debug, Clone, Deserialize)]
struct RawIndex {
    name: String,
    unique: i64,
    origin: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawIndexColumn {
    seqno: i64,
    name: Option<String>,
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

    async fn execute_csv_query(
        &self,
        path: &str,
        query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        self.execute_csv_query_with_display_query(path, query, query, source, read_only)
            .await
    }

    async fn execute_csv_query_with_display_query(
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
            .execute_csv(path, execution_query, read_only)
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let mut result = csv_to_query_result(execution_query, &stdout, source, elapsed)?;
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
        "INSERT" | "UPDATE" | "DELETE" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE"
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

fn is_dml_statement(statement: &str) -> bool {
    matches!(
        first_keyword(statement).to_ascii_uppercase().as_str(),
        "INSERT" | "UPDATE" | "DELETE"
    )
}

fn statement_returns_rows(statement: &str) -> bool {
    let keyword = first_keyword(statement);
    keyword.eq_ignore_ascii_case("SELECT")
        || keyword.eq_ignore_ascii_case("WITH")
        || (is_dml_statement(statement) && contains_keyword(statement, "RETURNING"))
}

fn count_result_statements(sql: &str) -> usize {
    split_sqlite_statements(sql)
        .into_iter()
        .filter(|stmt| statement_returns_rows(stmt))
        .count()
}

fn extract_last_csv_block<'a>(stdout: &'a str, sql: &str) -> &'a str {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(stdout.as_bytes());
    let mut record = csv::ByteRecord::new();
    let mut records = Vec::new();
    loop {
        let position = reader.position().clone();
        match reader.read_byte_record(&mut record) {
            Ok(true) => records.push((position.byte() as usize, record.clone())),
            Ok(false) => break,
            Err(_) => return stdout,
        }
    }

    if records.len() <= 2 {
        return stdout;
    }

    let expected_sets = count_result_statements(sql);
    let first_header = &records[0].1;
    let mut current_fc = first_header.len();
    let mut known_headers = vec![first_header.clone()];
    let mut last_header_idx = 0;
    let mut data_rows_since_header = 0usize;

    for (i, (_, record)) in records.iter().enumerate().skip(1) {
        let fc = record.len();
        if fc != current_fc {
            last_header_idx = i;
            current_fc = fc;
            data_rows_since_header = 0;
            if !known_headers.contains(record) {
                known_headers.push(record.clone());
            }
        } else if known_headers.contains(record) {
            last_header_idx = i;
            data_rows_since_header = 0;
        } else if expected_sets > 1
            && data_rows_since_header >= 1
            && known_headers.len() < expected_sets
        {
            last_header_idx = i;
            known_headers.push(record.clone());
            data_rows_since_header = 0;
        } else {
            data_rows_since_header += 1;
        }
    }

    if last_header_idx == 0 {
        return stdout;
    }

    &stdout[records[last_header_idx].0..]
}

fn csv_to_query_result(
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

    let csv_block = if count_result_statements(query) <= 1 {
        stdout
    } else {
        extract_last_csv_block(stdout, query)
    };
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_block.as_bytes());
    let columns = reader.headers()?.iter().map(ToString::to_string).collect();
    let mut rows = Vec::new();
    for result in reader.records() {
        rows.push(result?.iter().map(ToString::to_string).collect());
    }

    Ok(QueryResult::success(
        query.to_string(),
        columns,
        rows,
        execution_time_ms,
        source,
    ))
}

fn strip_sqlite_probes(
    stdout: &str,
    marker: &str,
) -> Result<(String, HashMap<usize, usize>), DbOperationError> {
    if stdout.trim().is_empty() {
        return Ok((String::new(), HashMap::new()));
    }

    let (stmt_col, changes_col) = sqlite_probe_columns(marker);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(stdout.as_bytes());
    let mut record = csv::ByteRecord::new();
    let mut records = Vec::new();
    while reader.read_byte_record(&mut record)? {
        records.push(record.clone());
    }

    let mut changes = HashMap::new();
    let mut kept = Vec::new();
    let mut removed_probe = false;
    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        if record.len() == 2
            && record.get(0) == Some(stmt_col.as_bytes())
            && record.get(1) == Some(changes_col.as_bytes())
        {
            removed_probe = true;
            let value = records.get(index + 1).ok_or_else(|| {
                DbOperationError::CommandTagParseFailed(
                    "missing SQLite statement probe row".to_string(),
                )
            })?;
            let stmt_index = value
                .get(0)
                .and_then(|raw| std::str::from_utf8(raw).ok())
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite statement probe index".to_string(),
                    )
                })?;
            let affected_rows = value
                .get(1)
                .and_then(|raw| std::str::from_utf8(raw).ok())
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite statement probe changes".to_string(),
                    )
                })?;
            changes.insert(stmt_index, affected_rows);
            index += 2;
        } else {
            kept.push(record.clone());
            index += 1;
        }
    }

    if !removed_probe {
        return Ok((stdout.to_string(), changes));
    }

    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_writer(Vec::new());
    for record in kept {
        writer.write_byte_record(&record)?;
    }
    let bytes = writer.into_inner().map_err(|error| {
        DbOperationError::QueryFailed(format!("Failed to write filtered SQLite CSV: {error}"))
    })?;
    String::from_utf8(bytes)
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
        .map(|filtered| (filtered, changes))
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

fn tcl_tag(query: &str) -> Option<CommandTag> {
    match first_keyword(query).to_ascii_uppercase().as_str() {
        "BEGIN" => Some(CommandTag::Begin),
        "COMMIT" | "END" => Some(CommandTag::Commit),
        "ROLLBACK" => Some(CommandTag::Rollback),
        "SAVEPOINT" => Some(CommandTag::Other("SAVEPOINT".to_string())),
        "RELEASE" => Some(CommandTag::Other("RELEASE".to_string())),
        _ => None,
    }
}

fn dml_tag(query: &str, affected_rows: usize) -> Option<CommandTag> {
    let affected_rows = affected_rows as u64;
    match first_keyword(query).to_ascii_uppercase().as_str() {
        "INSERT" => Some(CommandTag::Insert(affected_rows)),
        "UPDATE" => Some(CommandTag::Update(affected_rows)),
        "DELETE" => Some(CommandTag::Delete(affected_rows)),
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
                .or_else(|| tcl_tag(statement))
        })
        .collect()
}

fn discard_rolled_back(tags: &[CommandTag]) -> Vec<CommandTag> {
    let mut effective = Vec::new();
    let mut frames: Vec<Vec<CommandTag>> = Vec::new();

    for tag in tags {
        match tag {
            CommandTag::Begin => frames.push(Vec::new()),
            CommandTag::Other(raw) if raw == "SAVEPOINT" || raw.starts_with("SAVEPOINT ") => {
                frames.push(Vec::new());
            }
            CommandTag::Other(raw) if raw == "RELEASE" || raw.starts_with("RELEASE ") => {
                if frames.len() > 1
                    && let Some(inner) = frames.pop()
                {
                    if let Some(parent) = frames.last_mut() {
                        parent.extend(inner);
                    } else {
                        effective.extend(inner);
                    }
                }
            }
            CommandTag::Rollback => {
                if frames.len() > 1 {
                    frames.pop();
                } else {
                    frames.clear();
                }
            }
            CommandTag::Commit => {
                for frame in frames.drain(..) {
                    effective.extend(frame);
                }
            }
            _ => {
                if let Some(frame) = frames.last_mut() {
                    frame.push(tag.clone());
                } else {
                    effective.push(tag.clone());
                }
            }
        }
    }

    for frame in frames.drain(..) {
        effective.extend(frame);
    }

    effective
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
    let mut result =
        QueryResult::success(query.to_string(), Vec::new(), Vec::new(), elapsed, source);
    result.row_count = tag.affected_rows().unwrap_or(0) as usize;
    result.with_command_tag(tag)
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
        let query = sql::build_preview_query(table, &order_columns, limit, offset);
        self.execute_csv_query(path, &query, QuerySource::Preview, read_only)
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
            .execute_csv(path, &execution_query, read_only)
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let (stdout, changes) = strip_sqlite_probes(&stdout, &marker)?;
        let tag = aggregate_sqlite_command_tag(&sqlite_statement_tags(&statements, &changes));

        if stdout.trim().is_empty() {
            if let Some(tag) = tag {
                return Ok(command_tag_result(query, tag, elapsed, QuerySource::Adhoc));
            }
            return Ok(QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                QuerySource::Adhoc,
            ));
        }

        let mut result = csv_to_query_result(query, &stdout, QuerySource::Adhoc, elapsed)?;
        if let Some(tag) = tag {
            result = result.with_command_tag(tag);
        } else if statements.iter().any(|stmt| statement_returns_rows(stmt)) {
            let row_count = result.row_count as u64;
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
        self.cli
            .export_csv(Self::path_from_dsn(dsn)?, query, path, read_only)
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support::make_sqlite_db;
    use crate::app::ports::outbound::{MetadataProvider, QueryExecutor};
    use crate::domain::{CommandTag, QuerySource};

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
            assert_eq!(result.rows, vec![vec!["2".to_string(), "b".to_string()]]);
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
    }

    mod parsing {
        use super::*;

        #[test]
        fn csv_to_query_result_preserves_quoted_newline_for_single_statement() {
            let csv = "body,marker\n\"line 1\nline 2\",ok\n";

            let result =
                csv_to_query_result("SELECT body, marker FROM notes", csv, QuerySource::Adhoc, 1)
                    .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows,
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
        }

        #[test]
        fn csv_to_query_result_uses_last_result_set_for_multi_select() {
            let sqlite_csv_with_ignored_first_result_set =
                "ignored\n1\nbody,marker\n\"line 1\nline 2\",ok\n";

            let result = csv_to_query_result(
                "SELECT 1 AS ignored; SELECT body, marker FROM notes",
                sqlite_csv_with_ignored_first_result_set,
                QuerySource::Adhoc,
                1,
            )
            .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows,
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
        }

        #[test]
        fn parse_affected_rows_reads_trailing_changes_cell() {
            assert_eq!(parse_affected_rows("changes()\n3\n").unwrap(), 3);
        }

        #[test]
        fn strip_sqlite_probes_removes_probe_result_sets() {
            let marker = "probe";
            let stdout = "id,name\n1,Alice\nprobe_stmt,probe_changes\n0,2\nvalue\n42\n";

            let (filtered, changes) = strip_sqlite_probes(stdout, marker).unwrap();

            assert_eq!(changes.get(&0), Some(&2));
            assert_eq!(filtered, "id,name\n1,Alice\nvalue\n42\n");
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
            assert_eq!(result.rows, vec![vec!["1".to_string()]]);
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
                result.rows,
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
                result.rows,
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
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

            assert_eq!(result.row_count, 1);
            assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
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

            assert_eq!(result.row_count, 1);
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
            assert_eq!(result.rows, vec![vec!["x".to_string()]]);
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

            assert_eq!(result.row_count, 1);
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
            assert_eq!(result.row_count, 0);
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
            assert!(rows.rows.is_empty());
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
            assert_eq!(rows.rows, vec![vec!["1".to_string()]]);
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
            assert!(rows.rows.is_empty());
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
            assert!(rows.rows.is_empty());
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
            assert!(rows.rows.is_empty());
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

            assert_eq!(result.row_count, 1);
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
            assert_eq!(result.rows, vec![vec!["1".to_string(), "a".to_string()]]);
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

            assert_eq!(result.rows.len(), 2);
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

            assert_eq!(result.rows, vec![vec!["1".to_string(), "a".to_string()]]);
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

            assert_eq!(result.row_count, 1);
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

            assert_eq!(result.row_count, 1);
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

            assert_eq!(result.row_count, 1);
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
            assert_eq!(result.rows, vec![vec!["1".to_string()]]);
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
