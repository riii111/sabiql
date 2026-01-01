use crate::domain::Table;

#[derive(Debug, Clone)]
pub struct ErFkInfo {
    pub name: String,
    pub from_qualified: String,
    pub to_qualified: String,
}

#[derive(Debug, Clone)]
pub struct ErTableInfo {
    pub qualified_name: String,
    pub name: String,
    pub schema: String,
    pub foreign_keys: Vec<ErFkInfo>,
}

impl ErTableInfo {
    pub fn from_table(qualified_name: &str, table: &Table) -> Self {
        Self {
            qualified_name: qualified_name.to_string(),
            name: table.name.clone(),
            schema: table.schema.clone(),
            foreign_keys: table
                .foreign_keys
                .iter()
                .map(|fk| ErFkInfo {
                    name: fk.name.clone(),
                    from_qualified: format!("{}.{}", fk.from_schema, fk.from_table),
                    to_qualified: fk.referenced_table(),
                })
                .collect(),
        }
    }
}
