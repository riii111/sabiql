use crate::domain::{DatabaseMetadata, Schema, TableKindInfo, TableSummary};

use super::{
    super::{schema::MAIN_SCHEMA, sqlite3::metadata::RawTable},
    kind_info::{table_kind_info_from_legacy_sql, table_kind_info_from_raw},
};

pub(super) fn metadata_from_catalog(path: &str, tables: &[RawTable]) -> DatabaseMetadata {
    let mut metadata = DatabaseMetadata::new(database_name(path));
    metadata.schemas = vec![Schema::new(MAIN_SCHEMA)];
    metadata.table_summaries = tables
        .iter()
        .map(|table| {
            TableSummary::new(MAIN_SCHEMA.to_string(), table.name.clone(), None, false)
                .with_kind_info(kind_info_for_raw_table(table))
        })
        .collect();
    metadata
}

fn kind_info_for_raw_table(table: &RawTable) -> TableKindInfo {
    if table.kind.r#type.is_empty() {
        return table_kind_info_from_legacy_sql(table.kind.sql.as_deref());
    }
    table_kind_info_from_raw(&table.kind)
}

fn database_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
