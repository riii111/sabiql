use std::collections::{HashMap, HashSet};

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{
    Column, ColumnAttributes, FkAction, ForeignKey, Index, IndexAttributes, IndexType, Table,
    UNRESOLVED_FK_COLUMN,
};

use super::{
    super::{
        SqliteAdapter,
        schema::MAIN_SCHEMA,
        sql,
        sqlite3::metadata::{
            RawBatchIndex, RawColumn, RawForeignKey, RawIndexColumn, RawReferencedColumns,
            RawTableMetadata,
        },
    },
    kind_info::table_kind_info_from_raw,
    trigger::parse_sqlite_trigger,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TableDetailMode {
    Full,
    ColumnsAndFks,
    Signature,
}

impl TableDetailMode {
    const fn include_indexes(self) -> bool {
        matches!(self, Self::Full | Self::Signature)
    }

    const fn query_mode(self) -> sql::TableMetadataQueryMode {
        match self {
            Self::Full => sql::TableMetadataQueryMode::Full,
            Self::ColumnsAndFks => sql::TableMetadataQueryMode::ColumnsAndFks,
            Self::Signature => sql::TableMetadataQueryMode::FullWithoutRowCount,
        }
    }

    const fn include_triggers(self) -> bool {
        matches!(self, Self::Full | Self::Signature)
    }

    const fn include_source_ddl(self) -> bool {
        matches!(self, Self::Full | Self::Signature)
    }
}

pub(super) async fn fetch_table_detail(
    adapter: &SqliteAdapter,
    path: &str,
    table: &str,
    mode: TableDetailMode,
) -> Result<Table, DbOperationError> {
    let metadata = adapter
        .fetch_table_metadata_rows(path, table, mode.query_mode())
        .await?;
    table_from_metadata(table, mode, metadata)
}

pub(super) fn table_from_metadata(
    table: &str,
    mode: TableDetailMode,
    metadata: RawTableMetadata,
) -> Result<Table, DbOperationError> {
    if metadata.columns.is_empty() || metadata.table.is_none() {
        return Err(DbOperationError::ObjectMissing(format!(
            "SQLite table not found: {table}"
        )));
    }
    let unique_single_columns = unique_single_columns_from_batch(&metadata.indexes);
    let indexes = if mode.include_indexes() {
        indexes_from_batch(metadata.indexes)
    } else {
        Vec::new()
    };
    let mut raw_columns = metadata.columns;
    raw_columns.sort_by_key(|column| column.cid);
    let primary_key = extract_primary_key(&raw_columns);
    let columns: Vec<Column> = raw_columns
        .into_iter()
        .map(|column| {
            let is_pk = column.pk > 0;
            let is_hidden = column.hidden == 1;
            let is_generated = column.hidden == 2 || column.hidden == 3;
            let is_read_only = is_hidden || is_generated;
            let mut attributes = ColumnAttributes::from_parts(
                column.notnull == 0,
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
                character_set_name: None,
                collation_name: None,
                generation_expression: None,
                generation_kind: None,
            }
        })
        .collect();
    let primary_key = (!primary_key.is_empty()).then_some(primary_key);
    let kind_info = metadata
        .table
        .as_ref()
        .map(table_kind_info_from_raw)
        .unwrap_or_default();
    let foreign_keys =
        foreign_keys_from_batch(table, metadata.foreign_keys, &metadata.referenced_columns)?;
    let mut triggers = Vec::new();
    if mode.include_triggers() {
        for raw in metadata.triggers {
            if let Some(sql) = raw.sql {
                triggers.push(parse_sqlite_trigger(&raw.name, &sql)?);
            }
        }
        triggers.sort_by(|left, right| left.name.cmp(&right.name));
    }

    Ok(Table {
        schema: MAIN_SCHEMA.to_string(),
        name: table.to_string(),
        owner: None,
        columns,
        primary_key,
        foreign_keys,
        indexes,
        rls: None,
        triggers,
        row_count_estimate: metadata.row_count,
        comment: None,
        source_ddl: if mode.include_source_ddl() {
            metadata.source_ddl
        } else {
            None
        },
        kind_info,
    })
}

fn indexes_from_batch(raw_indexes: Vec<RawBatchIndex>) -> Vec<Index> {
    let mut indexes = raw_indexes
        .into_iter()
        .map(|raw| {
            let has_expression = raw
                .columns
                .iter()
                .any(|column| column.key != 0 && column.cid == -2);
            let has_auxiliary_columns = raw.columns.iter().any(|column| column.key == 0);
            let has_descending_key = raw
                .columns
                .iter()
                .any(|column| column.key != 0 && column.desc != 0);
            let has_non_binary_collation = raw.columns.iter().any(|column| {
                column.key != 0
                    && column
                        .coll
                        .as_deref()
                        .is_some_and(|collation| !collation.eq_ignore_ascii_case("BINARY"))
            });
            let columns = index_key_column_names(&raw.columns);
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
            Index {
                name: raw.name,
                columns,
                attributes,
                index_type: IndexType::Unknown,
                definition: raw.definition,
            }
        })
        .collect::<Vec<_>>();
    indexes.sort_by(|left, right| left.name.cmp(&right.name));
    indexes
}

fn unique_single_columns_from_batch(raw_indexes: &[RawBatchIndex]) -> HashSet<String> {
    raw_indexes
        .iter()
        .filter(|index| index.unique != 0 && index.partial == 0)
        .filter_map(|index| {
            let columns = index_key_column_names(&index.columns);
            (columns.len() == 1 && columns[0] != "<expression>").then(|| columns[0].clone())
        })
        .collect()
}

fn foreign_keys_from_batch(
    table: &str,
    mut raw: Vec<RawForeignKey>,
    referenced: &[RawReferencedColumns],
) -> Result<Vec<ForeignKey>, DbOperationError> {
    raw.sort_by_key(|fk| (fk.id, fk.seq));
    let referenced = referenced
        .iter()
        .map(|entry| (entry.name.as_str(), entry.columns.as_slice()))
        .collect::<HashMap<_, _>>();
    let mut grouped = Vec::new();
    let mut current: Option<ForeignKey> = None;
    let mut current_id = None;

    for fk in raw {
        let referenced_columns = referenced.get(fk.table.as_str()).copied();
        let (to_column, resolved) = if let Some(to) = &fk.to {
            let resolved = referenced_columns.is_some_and(|columns| {
                !columns.is_empty()
                    && columns
                        .iter()
                        .any(|column| column.name.eq_ignore_ascii_case(to))
            });
            (to.clone(), resolved)
        } else {
            let primary_key = referenced_columns.map(extract_primary_key);
            primary_key_target_column(&fk, primary_key.as_ref())
        };

        if current_id != Some(fk.id) {
            if let Some(foreign_key) = current.take() {
                grouped.push(foreign_key);
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
            current.reference_resolved &= resolved;
        }
    }
    if let Some(foreign_key) = current {
        grouped.push(foreign_key);
    }
    Ok(grouped)
}

pub(super) fn extract_primary_key(columns: &[RawColumn]) -> Vec<String> {
    let mut primary_key: Vec<(i64, String)> = columns
        .iter()
        .filter(|column| column.pk > 0)
        .map(|column| (column.pk, column.name.clone()))
        .collect();
    primary_key.sort_by_key(|(pk, _)| *pk);
    primary_key.into_iter().map(|(_, name)| name).collect()
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

fn parse_fk_action(action: &str) -> Result<FkAction, DbOperationError> {
    action
        .parse::<FkAction>()
        .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))
}

#[cfg(test)]
#[path = "table_detail_tests.rs"]
mod tests;
