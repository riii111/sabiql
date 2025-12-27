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
}

impl TableSummary {
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
