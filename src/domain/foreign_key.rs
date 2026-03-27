#[derive(Debug, Clone, PartialEq, Eq)]
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
            Self::NoAction => write!(f, "NO ACTION"),
            Self::Restrict => write!(f, "RESTRICT"),
            Self::Cascade => write!(f, "CASCADE"),
            Self::SetNull => write!(f, "SET NULL"),
            Self::SetDefault => write!(f, "SET DEFAULT"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referenced_table_returns_schema_dot_table() {
        let fk = ForeignKey {
            name: "fk_order_user".to_string(),
            from_schema: "public".to_string(),
            from_table: "orders".to_string(),
            from_columns: vec!["user_id".to_string()],
            to_schema: "public".to_string(),
            to_table: "users".to_string(),
            to_columns: vec!["id".to_string()],
            on_delete: FkAction::default(),
            on_update: FkAction::default(),
        };

        assert_eq!(fk.referenced_table(), "public.users");
    }
}
