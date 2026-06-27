use crate::domain::{TableObjectKind, TableStorage};

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct RawTableStorage {
    #[serde(rename = "type", default)]
    r#type: String,
    #[serde(default)]
    wr: i64,
    #[serde(default)]
    strict: i64,
    sql: Option<String>,
}

impl RawTableStorage {
    pub(super) fn into_table_storage(self) -> TableStorage {
        table_storage_from_pragma(&self.r#type, self.wr, self.strict, self.sql.as_deref())
    }
}

pub(super) fn table_storage_from_pragma(
    table_type: &str,
    without_rowid: i64,
    strict: i64,
    sql: Option<&str>,
) -> TableStorage {
    let mut storage = TableStorage {
        kind: if table_type == "virtual" {
            TableObjectKind::Virtual
        } else {
            TableObjectKind::Table
        },
        is_strict: strict != 0,
        without_rowid: without_rowid != 0,
        virtual_module: None,
    };
    if storage.kind == TableObjectKind::Virtual {
        storage.virtual_module = sql.and_then(parse_virtual_module);
    }
    enrich_storage_from_sql(storage, sql)
}

fn enrich_storage_from_sql(mut storage: TableStorage, sql: Option<&str>) -> TableStorage {
    let Some(sql) = sql else {
        return storage;
    };
    let upper = sql.to_ascii_uppercase();
    if storage.kind == TableObjectKind::Table
        && sql.to_ascii_lowercase().contains("create virtual table")
    {
        storage.kind = TableObjectKind::Virtual;
        storage.virtual_module = parse_virtual_module(sql);
    }
    if !storage.without_rowid && upper.contains("WITHOUT ROWID") {
        storage.without_rowid = true;
    }
    if !storage.is_strict && upper.contains(" STRICT") {
        storage.is_strict = true;
    }
    storage
}

fn parse_virtual_module(sql: &str) -> Option<String> {
    let lower = sql.to_ascii_lowercase();
    let using_idx = lower.find(" using ")?;
    let rest = sql[using_idx + 7..].trim_start();
    let module = rest
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()?
        .trim();
    if module.is_empty() {
        None
    } else {
        Some(module.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_virtual_module_from_ddl() {
        assert_eq!(
            parse_virtual_module("CREATE VIRTUAL TABLE notes_fts USING fts5(body);"),
            Some("fts5".to_string())
        );
    }

    #[test]
    fn legacy_sql_enriches_virtual_and_without_rowid() {
        let storage = table_storage_from_pragma(
            "table",
            0,
            0,
            Some("CREATE TABLE settings(key TEXT PRIMARY KEY) WITHOUT ROWID;"),
        );

        assert!(storage.without_rowid);
        assert_eq!(storage.kind, TableObjectKind::Table);
    }

    #[test]
    fn pragma_fields_mark_strict_virtual_table() {
        let storage = table_storage_from_pragma(
            "virtual",
            0,
            0,
            Some("CREATE VIRTUAL TABLE notes_fts USING fts5(body);"),
        );

        assert_eq!(storage.kind, TableObjectKind::Virtual);
        assert_eq!(storage.virtual_module.as_deref(), Some("fts5"));
    }
}
