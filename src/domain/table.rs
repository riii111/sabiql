use super::column::Column;
use super::foreign_key::ForeignKey;
use super::index::Index;
use super::rls::RlsInfo;

#[derive(Debug, Clone)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub primary_key: Option<Vec<String>>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<Index>,
    pub rls: Option<RlsInfo>,
    pub row_count_estimate: Option<i64>,
    pub comment: Option<String>,
}

impl Table {
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }

    pub fn display_name(&self, omit_public: bool) -> String {
        if omit_public && self.schema == "public" {
            self.name.clone()
        } else {
            self.qualified_name()
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableSummary {
    pub schema: String,
    pub name: String,
    pub row_count_estimate: Option<i64>,
    pub has_rls: bool,
    // Pre-computed for efficient case-insensitive filtering
    qualified_name_lower: String,
}

impl TableSummary {
    pub fn new(schema: String, name: String, row_count_estimate: Option<i64>, has_rls: bool) -> Self {
        let qualified_name_lower = format!("{}.{}", schema.to_lowercase(), name.to_lowercase());
        Self {
            schema,
            name,
            row_count_estimate,
            has_rls,
            qualified_name_lower,
        }
    }

    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }

    pub fn qualified_name_lower(&self) -> &str {
        &self.qualified_name_lower
    }

    pub fn display_name(&self, omit_public: bool) -> String {
        if omit_public && self.schema == "public" {
            self.name.clone()
        } else {
            self.qualified_name()
        }
    }
}
