use crate::domain::{Table, TableKindInfo};

pub fn minimal(schema: impl Into<String>, name: impl Into<String>) -> Table {
    Table {
        schema: schema.into(),
        name: name.into(),
        owner: None,
        columns: Vec::new(),
        primary_key: None,
        foreign_keys: Vec::new(),
        indexes: Vec::new(),
        rls: None,
        triggers: Vec::new(),
        row_count_estimate: None,
        comment: None,
        source_ddl: None,
        kind_info: TableKindInfo::default(),
    }
}

#[must_use]
pub fn table_fixture(configure: impl FnOnce(&mut Table)) -> Table {
    let mut table = minimal("", "");
    configure(&mut table);
    table
}
