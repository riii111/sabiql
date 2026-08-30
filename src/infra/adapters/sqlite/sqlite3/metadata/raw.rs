use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(in crate::adapters::sqlite) struct RawTable {
    pub(in crate::adapters::sqlite) name: String,
    #[serde(flatten)]
    pub(in crate::adapters::sqlite) kind: RawTableKindInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::adapters::sqlite) struct RawColumn {
    pub(in crate::adapters::sqlite) cid: i32,
    pub(in crate::adapters::sqlite) name: String,
    #[serde(rename = "type")]
    pub(in crate::adapters::sqlite) data_type: String,
    pub(in crate::adapters::sqlite) notnull: i64,
    pub(in crate::adapters::sqlite) dflt_value: Option<String>,
    pub(in crate::adapters::sqlite) pk: i64,
    #[serde(default)]
    pub(in crate::adapters::sqlite) hidden: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::adapters::sqlite) struct RawIndexColumn {
    pub(in crate::adapters::sqlite) cid: i64,
    pub(in crate::adapters::sqlite) name: Option<String>,
    #[serde(default)]
    pub(in crate::adapters::sqlite) desc: i64,
    #[serde(default)]
    pub(in crate::adapters::sqlite) coll: Option<String>,
    pub(in crate::adapters::sqlite) key: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::adapters::sqlite) struct RawForeignKey {
    pub(in crate::adapters::sqlite) id: i64,
    pub(in crate::adapters::sqlite) seq: i64,
    pub(in crate::adapters::sqlite) table: String,
    pub(in crate::adapters::sqlite) from: String,
    pub(in crate::adapters::sqlite) to: Option<String>,
    pub(in crate::adapters::sqlite) on_update: String,
    pub(in crate::adapters::sqlite) on_delete: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::adapters::sqlite) struct RawTrigger {
    pub(in crate::adapters::sqlite) name: String,
    pub(in crate::adapters::sqlite) sql: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RawRowCount {
    pub(super) count: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawJsonPayload {
    pub(super) payload: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::adapters::sqlite) struct RawTableKindInfo {
    #[serde(rename = "type", default)]
    pub(in crate::adapters::sqlite) r#type: String,
    #[serde(default)]
    pub(in crate::adapters::sqlite) wr: i64,
    #[serde(default)]
    pub(in crate::adapters::sqlite) strict: i64,
    pub(in crate::adapters::sqlite) sql: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::adapters::sqlite) struct RawPreviewMetadata {
    #[serde(default)]
    pub(in crate::adapters::sqlite) columns: Vec<RawColumn>,
    pub(in crate::adapters::sqlite) table: Option<RawTableKindInfo>,
}

#[derive(Debug, Deserialize)]
pub(in crate::adapters::sqlite) struct RawBatchIndex {
    pub(in crate::adapters::sqlite) name: String,
    pub(in crate::adapters::sqlite) unique: i64,
    #[serde(default)]
    pub(in crate::adapters::sqlite) origin: String,
    #[serde(default)]
    pub(in crate::adapters::sqlite) partial: i64,
    #[serde(default)]
    pub(in crate::adapters::sqlite) columns: Vec<RawIndexColumn>,
    pub(in crate::adapters::sqlite) definition: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::adapters::sqlite) struct RawReferencedColumns {
    pub(in crate::adapters::sqlite) name: String,
    #[serde(default)]
    pub(in crate::adapters::sqlite) columns: Vec<RawColumn>,
}

#[derive(Debug, Deserialize)]
pub(in crate::adapters::sqlite) struct RawTableMetadata {
    pub(in crate::adapters::sqlite) table: Option<RawTableKindInfo>,
    #[serde(default)]
    pub(in crate::adapters::sqlite) columns: Vec<RawColumn>,
    #[serde(default)]
    pub(in crate::adapters::sqlite) indexes: Vec<RawBatchIndex>,
    #[serde(default)]
    pub(in crate::adapters::sqlite) foreign_keys: Vec<RawForeignKey>,
    #[serde(default)]
    pub(in crate::adapters::sqlite) triggers: Vec<RawTrigger>,
    #[serde(default)]
    pub(in crate::adapters::sqlite) referenced_columns: Vec<RawReferencedColumns>,
    pub(in crate::adapters::sqlite) row_count: Option<i64>,
    pub(in crate::adapters::sqlite) source_ddl: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawNamedJsonPayload {
    pub(super) name: String,
    pub(super) payload: String,
}

pub(in crate::adapters::sqlite) struct RawNamedTableMetadata {
    pub(in crate::adapters::sqlite) name: String,
    pub(in crate::adapters::sqlite) metadata: RawTableMetadata,
}
