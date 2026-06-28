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
