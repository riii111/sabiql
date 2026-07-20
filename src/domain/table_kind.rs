#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableKind {
    #[default]
    Table,
    Virtual,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableKindInfo {
    pub kind: TableKind,
    pub is_strict: bool,
    pub without_rowid: bool,
    pub virtual_module: Option<String>,
}
