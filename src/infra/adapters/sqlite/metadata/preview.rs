use crate::app::ports::outbound::DbOperationError;
use crate::domain::TableKindInfo;

use super::{
    super::{SqliteAdapter, sqlite3::metadata::RawPreviewMetadata},
    kind_info::table_kind_info_from_raw,
    table_detail::extract_primary_key,
};

impl SqliteAdapter {
    pub(in crate::adapters::sqlite) async fn preview_metadata(
        &self,
        path: &str,
        table: &str,
    ) -> Result<(Vec<String>, Vec<String>, TableKindInfo), DbOperationError> {
        let metadata = self.fetch_preview_metadata_rows(path, table).await?;
        preview_metadata_from_raw(table, metadata)
    }
}

fn preview_metadata_from_raw(
    table: &str,
    metadata: RawPreviewMetadata,
) -> Result<(Vec<String>, Vec<String>, TableKindInfo), DbOperationError> {
    let Some(table_kind) = metadata.table.as_ref() else {
        return Err(DbOperationError::ObjectMissing(format!(
            "SQLite table not found: {table}"
        )));
    };
    if metadata.columns.is_empty() {
        return Err(DbOperationError::ObjectMissing(format!(
            "SQLite table not found: {table}"
        )));
    }
    let primary_key = extract_primary_key(&metadata.columns);
    let visible_columns = metadata
        .columns
        .into_iter()
        .filter(|column| column.hidden != 1)
        .map(|column| column.name)
        .collect();
    Ok((
        visible_columns,
        primary_key,
        table_kind_info_from_raw(table_kind),
    ))
}
