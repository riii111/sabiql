#[derive(Debug, Clone)]
pub struct ForeignKey {
    pub name: String,
    pub from_schema: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_schema: String,
    pub to_table: String,
    pub to_columns: Vec<String>,
    pub on_delete: FkAction,
    pub on_update: FkAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FkAction {
    #[default]
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

impl ForeignKey {
    pub fn referenced_table(&self) -> String {
        format!("{}.{}", self.to_schema, self.to_table)
    }
}

impl std::fmt::Display for FkAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FkAction::NoAction => write!(f, "NO ACTION"),
            FkAction::Restrict => write!(f, "RESTRICT"),
            FkAction::Cascade => write!(f, "CASCADE"),
            FkAction::SetNull => write!(f, "SET NULL"),
            FkAction::SetDefault => write!(f, "SET DEFAULT"),
        }
    }
}
