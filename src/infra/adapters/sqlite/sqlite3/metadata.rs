use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use serde::Deserialize;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{
    Column, ColumnAttributes, DatabaseMetadata, FkAction, ForeignKey, Index, IndexAttributes,
    IndexType, Schema, Table, TableSignature, TableSummary, Trigger, TriggerEvent, TriggerTiming,
    UNRESOLVED_FK_COLUMN,
};

use super::super::{MAIN_SCHEMA, SqliteAdapter, sql};

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
    #[serde(default)]
    desc: i64,
    #[serde(default)]
    coll: Option<String>,
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
struct RawTrigger {
    name: String,
    sql: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRowCount {
    count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableDetailMode {
    Full,
    ColumnsAndFks,
    Signature,
}

impl TableDetailMode {
    const fn include_indexes(self) -> bool {
        matches!(self, Self::Full | Self::Signature)
    }

    const fn include_row_count(self) -> bool {
        matches!(self, Self::Full)
    }
}

impl SqliteAdapter {
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

    pub(in crate::adapters::sqlite) async fn preview_visible_column_names(
        &self,
        path: &str,
        table: &str,
    ) -> Result<Vec<String>, DbOperationError> {
        Ok(self
            .columns(path, table)
            .await?
            .into_iter()
            .filter(|column| column.hidden != 1)
            .map(|column| column.name)
            .collect())
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

    pub(in crate::adapters::sqlite) fn validate_main_schema(
        schema: &str,
    ) -> Result<(), DbOperationError> {
        if schema == MAIN_SCHEMA {
            Ok(())
        } else {
            Err(DbOperationError::ObjectMissing(format!(
                "SQLite schema not found: {schema}"
            )))
        }
    }

    pub(in crate::adapters::sqlite) async fn preview_order_columns(
        &self,
        path: &str,
        table: &str,
    ) -> Vec<String> {
        self.primary_key_columns(path, table)
            .await
            .unwrap_or_default()
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
            let has_descending_key = columns.iter().any(|col| col.key != 0 && col.desc != 0);
            let has_non_binary_collation = columns.iter().any(|col| {
                col.key != 0
                    && col
                        .coll
                        .as_deref()
                        .is_some_and(|collation| !collation.eq_ignore_ascii_case("BINARY"))
            });
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
            if has_descending_key {
                attributes = attributes | IndexAttributes::DESCENDING;
            }
            if has_non_binary_collation {
                attributes = attributes | IndexAttributes::NON_BINARY_COLLATION;
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

    async fn triggers(&self, path: &str, table: &str) -> Result<Vec<Trigger>, DbOperationError> {
        let raw: Vec<RawTrigger> = self
            .cli
            .execute_json(path, &sql::trigger_list_query(table))
            .await?;
        let mut triggers = Vec::new();

        for raw in raw {
            if let Some(sql) = raw.sql {
                triggers.push(parse_sqlite_trigger(&raw.name, &sql)?);
            }
        }

        triggers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(triggers)
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
        let mut referenced_columns = HashMap::new();

        for fk in raw {
            let (to_column, resolved) = self
                .resolve_fk_target_column(
                    path,
                    &fk,
                    &mut referenced_primary_keys,
                    &mut referenced_columns,
                )
                .await?;

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
                    reference_resolved: resolved,
                });
            }

            if let Some(current) = &mut current {
                current.from_columns.push(fk.from);
                current.to_columns.push(to_column);
                if !resolved {
                    current.reference_resolved = false;
                }
            }
        }

        if let Some(fk) = current {
            grouped.push(fk);
        }

        Ok(grouped)
    }

    async fn resolve_fk_target_column(
        &self,
        path: &str,
        fk: &RawForeignKey,
        referenced_primary_keys: &mut HashMap<String, Vec<String>>,
        referenced_columns: &mut HashMap<String, HashSet<String>>,
    ) -> Result<(String, bool), DbOperationError> {
        if let Some(to) = &fk.to {
            self.cache_table_columns(path, &fk.table, referenced_columns)
                .await?;
            if !Self::cached_table_has_columns(referenced_columns, &fk.table) {
                return Ok((to.clone(), false));
            }
            let resolved = referenced_columns.get(&fk.table).is_some_and(|columns| {
                columns.iter().any(|column| column.eq_ignore_ascii_case(to))
            });
            Ok((to.clone(), resolved))
        } else if !referenced_primary_keys.contains_key(&fk.table) {
            let columns = self.columns(path, &fk.table).await?;
            referenced_primary_keys.insert(fk.table.clone(), Self::extract_primary_key(&columns));
            Ok(Self::primary_key_target_column(
                fk,
                referenced_primary_keys.get(&fk.table),
            ))
        } else {
            Ok(Self::primary_key_target_column(
                fk,
                referenced_primary_keys.get(&fk.table),
            ))
        }
    }

    fn primary_key_target_column(
        fk: &RawForeignKey,
        primary_key: Option<&Vec<String>>,
    ) -> (String, bool) {
        let Some(primary_key) = primary_key.filter(|columns| !columns.is_empty()) else {
            return (UNRESOLVED_FK_COLUMN.to_string(), false);
        };
        match usize::try_from(fk.seq)
            .ok()
            .and_then(|idx| primary_key.get(idx))
        {
            Some(column) => (column.clone(), true),
            None => (UNRESOLVED_FK_COLUMN.to_string(), false),
        }
    }

    async fn cache_table_columns(
        &self,
        path: &str,
        table: &str,
        cache: &mut HashMap<String, HashSet<String>>,
    ) -> Result<(), DbOperationError> {
        if !cache.contains_key(table) {
            let columns = self.columns(path, table).await?;
            cache.insert(
                table.to_string(),
                columns.into_iter().map(|column| column.name).collect(),
            );
        }
        Ok(())
    }

    fn cached_table_has_columns(cache: &HashMap<String, HashSet<String>>, table: &str) -> bool {
        !cache.get(table).is_some_and(HashSet::is_empty)
    }

    async fn table_detail_with_mode(
        &self,
        path: &str,
        table: &str,
        mode: TableDetailMode,
    ) -> Result<Table, DbOperationError> {
        let include_indexes = mode.include_indexes();
        let include_row_count = mode.include_row_count();
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
            triggers: self.triggers(path, table).await?,
            row_count_estimate: if include_row_count {
                self.row_count(path, table).await
            } else {
                None
            },
            comment: None,
            source_ddl: self.table_definition(path, table).await,
        })
    }

    async fn signature_for_table(
        &self,
        path: &str,
        table: &RawTable,
    ) -> Result<TableSignature, DbOperationError> {
        let detail = self
            .table_detail_with_mode(path, &table.name, TableDetailMode::Signature)
            .await?;
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
                "idx={}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                index.name,
                index.columns.join(","),
                index.is_unique(),
                index.is_primary(),
                index.is_partial(),
                index.has_expression(),
                index.has_auxiliary_columns(),
                index.has_descending_key(),
                index.has_non_binary_collation(),
                index.definition.clone().unwrap_or_default()
            )
        }));
        parts.extend(detail.foreign_keys.iter().map(|fk| {
            format!(
                "fk={}:{}:{}:{}:{}:{}:{}",
                fk.name,
                fk.from_columns.join(","),
                fk.to_table,
                fk.to_columns.join(","),
                fk.on_delete,
                fk.on_update,
                fk.reference_resolved
            )
        }));
        parts.extend(detail.triggers.iter().map(|trigger| {
            format!(
                "trg={}:{}:{}:{}",
                trigger.name,
                trigger.timing,
                trigger
                    .events
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                trigger.function_name
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

fn sqlite_trigger_parse_error(sql: &str, detail: &str) -> DbOperationError {
    DbOperationError::MetadataParseFailed(format!(
        "sqlite trigger parse failed ({detail}): {}",
        sql.chars().take(120).collect::<String>()
    ))
}

fn skip_optional_if_not_exists(sql: &str, pos: usize) -> usize {
    let Some((keyword, next)) = next_keyword_from(sql, pos) else {
        return pos;
    };
    if !keyword.eq_ignore_ascii_case("IF") {
        return pos;
    }
    let Some((not, next)) = next_keyword_from(sql, next) else {
        return pos;
    };
    if !not.eq_ignore_ascii_case("NOT") {
        return pos;
    }
    let Some((exists, next)) = next_keyword_from(sql, next) else {
        return pos;
    };
    if exists.eq_ignore_ascii_case("EXISTS") {
        next
    } else {
        pos
    }
}

fn skip_object_reference(sql: &str, pos: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut i = pos;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = match bytes.get(i) {
        Some(b'"' | b'\'' | b'`') => skip_quoted(bytes, i, bytes[i]),
        Some(b'[') => skip_bracket_quoted(bytes, i),
        Some(b) if b.is_ascii_alphanumeric() || *b == b'_' => {
            let Some((_, end)) = next_keyword_from(sql, i) else {
                return i;
            };
            end
        }
        _ => return i,
    };
    if bytes.get(start) == Some(&b'.')
        && let Some((_, end)) = next_keyword_from(sql, start + 1)
    {
        return end;
    }
    start
}

fn skip_update_of_clause(sql: &str, pos: usize) -> usize {
    let Some((keyword, next)) = next_keyword_from(sql, pos) else {
        return pos;
    };
    if !keyword.eq_ignore_ascii_case("OF") {
        return pos;
    }

    let mut pos = next;
    loop {
        let Some((keyword, next)) = next_keyword_from(sql, pos) else {
            return pos;
        };
        match keyword.to_ascii_uppercase().as_str() {
            "INSERT" | "UPDATE" | "DELETE" | "ON" | "FOR" | "WHEN" | "BEGIN" => return pos,
            _ => pos = next,
        }
    }
}

fn parse_sqlite_trigger_events(
    sql: &str,
    pos: usize,
) -> Result<(Vec<TriggerEvent>, usize), DbOperationError> {
    let mut events = Vec::new();
    let mut pos = pos;
    loop {
        let Some((keyword, next)) = next_keyword_from(sql, pos) else {
            return Err(sqlite_trigger_parse_error(sql, "missing trigger event"));
        };
        if keyword.eq_ignore_ascii_case("ON") {
            break;
        }

        match keyword.to_ascii_uppercase().as_str() {
            "INSERT" => events.push(TriggerEvent::Insert),
            "UPDATE" => {
                events.push(TriggerEvent::Update);
                pos = skip_update_of_clause(sql, next);
                continue;
            }
            "DELETE" => events.push(TriggerEvent::Delete),
            _ => return Err(sqlite_trigger_parse_error(sql, "unsupported trigger event")),
        }
        pos = next;
    }

    if events.is_empty() {
        return Err(sqlite_trigger_parse_error(sql, "no trigger events"));
    }

    Ok((events, pos))
}

fn parse_sqlite_trigger_header(
    sql: &str,
    pos: usize,
) -> Result<(TriggerTiming, Vec<TriggerEvent>, usize), DbOperationError> {
    let Some((keyword, next)) = next_keyword_from(sql, pos) else {
        return Err(sqlite_trigger_parse_error(
            sql,
            "missing trigger timing or event",
        ));
    };

    match keyword.to_ascii_uppercase().as_str() {
        "BEFORE" => {
            let (events, pos) = parse_sqlite_trigger_events(sql, next)?;
            Ok((TriggerTiming::Before, events, pos))
        }
        "AFTER" => {
            let (events, pos) = parse_sqlite_trigger_events(sql, next)?;
            Ok((TriggerTiming::After, events, pos))
        }
        "INSTEAD" => {
            let Some((of, next)) = next_keyword_from(sql, next) else {
                return Err(sqlite_trigger_parse_error(sql, "incomplete INSTEAD OF"));
            };
            if !of.eq_ignore_ascii_case("OF") {
                return Err(sqlite_trigger_parse_error(sql, "expected OF after INSTEAD"));
            }
            let (events, pos) = parse_sqlite_trigger_events(sql, next)?;
            Ok((TriggerTiming::InsteadOf, events, pos))
        }
        "INSERT" | "UPDATE" | "DELETE" => {
            let (events, pos) = parse_sqlite_trigger_events(sql, pos)?;
            Ok((TriggerTiming::Before, events, pos))
        }
        _ => Err(sqlite_trigger_parse_error(
            sql,
            "unsupported trigger timing or event",
        )),
    }
}

fn parse_sqlite_trigger(trigger_name: &str, sql: &str) -> Result<Trigger, DbOperationError> {
    let Some((first, pos)) = next_keyword_from(sql, 0) else {
        return Err(sqlite_trigger_parse_error(sql, "missing CREATE"));
    };
    if !first.eq_ignore_ascii_case("CREATE") {
        return Err(sqlite_trigger_parse_error(sql, "expected CREATE"));
    }
    let Some((second, pos)) = next_keyword_from(sql, pos) else {
        return Err(sqlite_trigger_parse_error(sql, "missing TRIGGER"));
    };
    if !second.eq_ignore_ascii_case("TRIGGER") {
        return Err(sqlite_trigger_parse_error(sql, "expected TRIGGER"));
    }

    let mut pos = pos;
    if let Some((keyword, next)) = next_keyword_from(sql, pos)
        && keyword.eq_ignore_ascii_case("TEMP")
    {
        pos = next;
    }
    pos = skip_optional_if_not_exists(sql, pos);
    pos = skip_object_reference(sql, pos);

    let (timing, events, _) = parse_sqlite_trigger_header(sql, pos)?;

    Ok(Trigger {
        name: trigger_name.to_string(),
        timing,
        events,
        function_name: sql.to_string(),
        security_definer: false,
    })
}

fn parse_fk_action(action: &str) -> Result<FkAction, DbOperationError> {
    action
        .parse::<FkAction>()
        .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))
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
                None,
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
        self.table_detail_with_mode(Self::path_from_dsn(dsn)?, table, TableDetailMode::Full)
            .await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        Self::validate_main_schema(schema)?;
        self.table_detail_with_mode(
            Self::path_from_dsn(dsn)?,
            table,
            TableDetailMode::ColumnsAndFks,
        )
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
    use crate::adapters::test_support::make_sqlite_db;
    use crate::app::ports::outbound::{DdlGenerator, MetadataProvider, QueryExecutor};
    use crate::domain::{DatabaseType, FkAction, IndexType, Schema, UNRESOLVED_FK_COLUMN};

    use super::*;

    #[test]
    fn index_key_column_names_preserves_expression_and_unknown_key_columns() {
        let columns = vec![
            RawIndexColumn {
                seqno: 0,
                cid: 1,
                name: Some("email".to_string()),
                desc: 0,
                coll: None,
                key: 1,
            },
            RawIndexColumn {
                seqno: 1,
                cid: -2,
                name: None,
                desc: 0,
                coll: None,
                key: 1,
            },
            RawIndexColumn {
                seqno: 2,
                cid: 99,
                name: None,
                desc: 0,
                coll: None,
                key: 1,
            },
            RawIndexColumn {
                seqno: 3,
                cid: 2,
                name: Some("rowid".to_string()),
                desc: 0,
                coll: None,
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

    mod metadata {
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
            assert!(metadata.table_summaries[0].row_count_estimate.is_none());
        }

        #[tokio::test]
        async fn skips_row_count_even_when_table_has_rows() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            INSERT INTO users(id) VALUES (1), (2), (3);
            ",
            );
            let adapter = SqliteAdapter::new();

            let metadata = adapter.fetch_metadata(&dsn).await.unwrap();

            assert_eq!(metadata.table_summaries.len(), 1);
            assert!(metadata.table_summaries[0].row_count_estimate.is_none());
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
        async fn columns_and_fks_skips_row_count() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            INSERT INTO users(id) VALUES (1), (2), (3);
            ",
            );
            let adapter = SqliteAdapter::new();

            let light = adapter
                .fetch_table_columns_and_fks(&dsn, "main", "users")
                .await
                .unwrap();
            let full = adapter
                .fetch_table_detail(&dsn, "main", "users")
                .await
                .unwrap();

            assert!(light.row_count_estimate.is_none());
            assert_eq!(full.row_count_estimate, Some(3));
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
            assert!(index.needs_source_definition_detail());
            assert!(index.definition.as_deref().is_some_and(|definition| {
                definition.contains("lower(email)")
                    && definition.contains("WHERE email IS NOT NULL")
            }));
        }

        #[tokio::test]
        async fn partial_index_preserves_where_clause_in_definition() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(email TEXT);
            CREATE INDEX idx_users_email_active
                ON users(email)
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
                .find(|index| index.name == "idx_users_email_active")
                .unwrap();

            assert_eq!(index.columns, vec!["email".to_string()]);
            assert!(index.is_partial());
            assert!(index.needs_source_definition_detail());
            assert!(
                index
                    .definition
                    .as_deref()
                    .is_some_and(|definition| { definition.contains("WHERE email IS NOT NULL") })
            );
        }

        #[tokio::test]
        async fn descending_and_collation_indexes_preserve_definition() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(name TEXT, created_at TEXT);
            CREATE INDEX idx_users_name_desc ON users(name DESC);
            CREATE INDEX idx_users_name_nocase ON users(name COLLATE NOCASE);
            ",
            );
            let adapter = SqliteAdapter::new();

            let detail = adapter
                .fetch_table_detail(&dsn, "main", "users")
                .await
                .unwrap();

            let descending = detail
                .indexes
                .iter()
                .find(|index| index.name == "idx_users_name_desc")
                .unwrap();
            assert!(descending.has_descending_key());
            assert!(descending.needs_source_definition_detail());
            assert!(
                descending
                    .definition
                    .as_deref()
                    .is_some_and(|definition| { definition.contains("DESC") })
            );

            let collation = detail
                .indexes
                .iter()
                .find(|index| index.name == "idx_users_name_nocase")
                .unwrap();
            assert!(collation.has_non_binary_collation());
            assert!(collation.needs_source_definition_detail());
            assert!(
                collation
                    .definition
                    .as_deref()
                    .is_some_and(|definition| { definition.contains("COLLATE NOCASE") })
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
            assert!(detail.foreign_keys[0].reference_resolved);
        }

        #[tokio::test]
        async fn foreign_key_to_missing_table_is_kept_as_unresolved() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            PRAGMA foreign_keys=OFF;
            CREATE TABLE child(
                org_id INTEGER REFERENCES missing_orgs(id)
            );
            ",
            );
            let adapter = SqliteAdapter::new();

            let detail = adapter
                .fetch_table_detail(&dsn, "main", "child")
                .await
                .unwrap();

            assert_eq!(detail.columns.len(), 1);
            assert_eq!(detail.foreign_keys.len(), 1);
            assert_eq!(detail.foreign_keys[0].to_table, "missing_orgs");
            assert_eq!(detail.foreign_keys[0].to_columns, vec!["id".to_string()]);
            assert!(!detail.foreign_keys[0].reference_resolved);
        }

        #[tokio::test]
        async fn foreign_key_to_missing_column_is_kept_as_unresolved() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            PRAGMA foreign_keys=OFF;
            CREATE TABLE parent(a INTEGER PRIMARY KEY);
            CREATE TABLE child(
                x INTEGER REFERENCES parent(missing_col)
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
                detail.foreign_keys[0].to_columns,
                vec!["missing_col".to_string()]
            );
            assert!(!detail.foreign_keys[0].reference_resolved);
        }

        #[tokio::test]
        async fn foreign_key_without_target_columns_and_missing_parent_pk_is_unresolved() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            PRAGMA foreign_keys=OFF;
            CREATE TABLE parent(a INTEGER);
            CREATE TABLE child(x INTEGER REFERENCES parent);
            ",
            );
            let adapter = SqliteAdapter::new();

            let detail = adapter
                .fetch_table_detail(&dsn, "main", "child")
                .await
                .unwrap();

            assert_eq!(detail.foreign_keys.len(), 1);
            assert_eq!(
                detail.foreign_keys[0].to_columns,
                vec![UNRESOLVED_FK_COLUMN.to_string()]
            );
            assert!(!detail.foreign_keys[0].reference_resolved);
        }

        #[tokio::test]
        async fn foreign_key_target_column_matches_case_insensitively() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE parent(id INTEGER PRIMARY KEY);
            CREATE TABLE child(x INTEGER REFERENCES parent(ID));
            ",
            );
            let adapter = SqliteAdapter::new();

            let detail = adapter
                .fetch_table_detail(&dsn, "main", "child")
                .await
                .unwrap();

            assert_eq!(detail.foreign_keys.len(), 1);
            assert_eq!(detail.foreign_keys[0].to_columns, vec!["ID".to_string()]);
            assert!(detail.foreign_keys[0].reference_resolved);
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

        #[tokio::test]
        async fn unresolved_foreign_key_is_included_in_signature() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            PRAGMA foreign_keys=OFF;
            CREATE TABLE child(
                org_id INTEGER REFERENCES missing_orgs(id)
            );
            ",
            );
            let adapter = SqliteAdapter::new();

            let signatures = adapter.fetch_table_signatures(&dsn).await.unwrap();
            let signature = signatures
                .iter()
                .find(|signature| signature.name == "child")
                .expect("child table signature");

            assert!(
                signature
                    .signature
                    .contains("fk=fk_child_0:org_id:missing_orgs:id:NO ACTION:NO ACTION:false")
            );
        }

        #[tokio::test]
        async fn index_desc_and_collation_change_signature() {
            let adapter = SqliteAdapter::new();
            let (_asc_dir, asc_dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(name TEXT);
            CREATE INDEX idx_users_name ON users(name);
            ",
            );
            let (_desc_dir, desc_dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(name TEXT);
            CREATE INDEX idx_users_name ON users(name DESC);
            ",
            );
            let (_binary_dir, binary_dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(name TEXT);
            CREATE INDEX idx_users_name ON users(name);
            ",
            );
            let (_nocase_dir, nocase_dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(name TEXT);
            CREATE INDEX idx_users_name ON users(name COLLATE NOCASE);
            ",
            );

            let asc_signature = adapter
                .fetch_table_signatures(&asc_dsn)
                .await
                .unwrap()
                .into_iter()
                .find(|signature| signature.name == "users")
                .unwrap()
                .signature;
            let desc_signature = adapter
                .fetch_table_signatures(&desc_dsn)
                .await
                .unwrap()
                .into_iter()
                .find(|signature| signature.name == "users")
                .unwrap()
                .signature;
            let binary_signature = adapter
                .fetch_table_signatures(&binary_dsn)
                .await
                .unwrap()
                .into_iter()
                .find(|signature| signature.name == "users")
                .unwrap()
                .signature;
            let nocase_signature = adapter
                .fetch_table_signatures(&nocase_dsn)
                .await
                .unwrap()
                .into_iter()
                .find(|signature| signature.name == "users")
                .unwrap()
                .signature;

            assert_ne!(asc_signature, desc_signature);
            assert!(
                desc_signature.contains(
                    "idx=idx_users_name:name:false:false:false:false:true:true:false:CREATE INDEX idx_users_name ON users(name DESC)"
                )
            );
            assert_ne!(binary_signature, nocase_signature);
            assert!(
                nocase_signature.contains(
                    "idx=idx_users_name:name:false:false:false:false:true:false:true:CREATE INDEX idx_users_name ON users(name COLLATE NOCASE)"
                )
            );
        }

        #[tokio::test]
        async fn trigger_change_updates_signature() {
            let setup = r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            CREATE TABLE audit(user_id INTEGER);
            ";
            let trigger = r"
            CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN
                INSERT INTO audit(user_id) VALUES (new.id);
            END;
            ";
            let (_dir, dsn) = make_sqlite_db(setup);
            let adapter = SqliteAdapter::new();

            let before = adapter.fetch_table_signatures(&dsn).await.unwrap();
            let before_signature = before
                .iter()
                .find(|signature| signature.name == "users")
                .unwrap()
                .signature
                .clone();

            adapter.execute_adhoc(&dsn, trigger, false).await.unwrap();

            let after = adapter.fetch_table_signatures(&dsn).await.unwrap();
            let after_signature = &after
                .iter()
                .find(|signature| signature.name == "users")
                .unwrap()
                .signature;

            assert_ne!(before_signature, after_signature.as_str());
            assert!(
                after_signature.contains("trg=users_audit:AFTER:INSERT:CREATE TRIGGER users_audit")
            );
        }
    }

    mod trigger_parsing {
        use super::*;

        #[test]
        fn parses_after_insert_trigger() {
            let sql = "CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END";
            let trigger = parse_sqlite_trigger("users_audit", sql).unwrap();

            assert_eq!(trigger.name, "users_audit");
            assert_eq!(trigger.timing, TriggerTiming::After);
            assert_eq!(trigger.events, vec![TriggerEvent::Insert]);
            assert_eq!(trigger.function_name, sql);
            assert!(!trigger.security_definer);
        }

        #[test]
        fn parses_before_update_of_columns() {
            let sql =
                "CREATE TRIGGER users_guard BEFORE UPDATE OF name ON users BEGIN SELECT 1; END";
            let trigger = parse_sqlite_trigger("users_guard", sql).unwrap();

            assert_eq!(trigger.timing, TriggerTiming::Before);
            assert_eq!(trigger.events, vec![TriggerEvent::Update]);
        }

        #[test]
        fn parses_instead_of_delete_trigger() {
            let sql =
                "CREATE TRIGGER users_view_io INSTEAD OF DELETE ON users_view BEGIN SELECT 1; END";
            let trigger = parse_sqlite_trigger("users_view_io", sql).unwrap();

            assert_eq!(trigger.timing, TriggerTiming::InsteadOf);
            assert_eq!(trigger.events, vec![TriggerEvent::Delete]);
        }

        #[test]
        fn omitted_timing_defaults_to_before() {
            let sql = "CREATE TRIGGER users_log INSERT ON users BEGIN SELECT 1; END";
            let trigger = parse_sqlite_trigger("users_log", sql).unwrap();

            assert_eq!(trigger.timing, TriggerTiming::Before);
            assert_eq!(trigger.events, vec![TriggerEvent::Insert]);
        }
    }

    mod trigger_metadata {
        use super::*;

        #[tokio::test]
        async fn table_detail_loads_trigger_without_explicit_timing() {
            let (_dir, dsn) = make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            CREATE TRIGGER users_log INSERT ON users BEGIN SELECT 1; END;
            ",
            );
            let adapter = SqliteAdapter::new();

            let detail = adapter
                .fetch_table_detail(&dsn, "main", "users")
                .await
                .unwrap();

            assert_eq!(detail.triggers.len(), 1);
            assert_eq!(detail.triggers[0].name, "users_log");
            assert_eq!(detail.triggers[0].timing, TriggerTiming::Before);
            assert_eq!(detail.triggers[0].events, vec![TriggerEvent::Insert]);
        }
    }
}
