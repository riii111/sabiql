#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableObjectKind {
    #[default]
    Table,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableStorage {
    pub kind: TableObjectKind,
    pub is_strict: bool,
    pub without_rowid: bool,
    pub virtual_module: Option<String>,
}

impl TableStorage {
    #[must_use]
    pub const fn regular_table() -> Self {
        Self {
            kind: TableObjectKind::Table,
            is_strict: false,
            without_rowid: false,
            virtual_module: None,
        }
    }
}
