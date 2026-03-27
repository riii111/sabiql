use super::column::Column;
use super::foreign_key::ForeignKey;
use super::index::Index;
use super::rls::RlsInfo;
use super::trigger::Trigger;

fn make_qualified_name(schema: &str, name: &str) -> String {
    format!("{schema}.{name}")
}

fn make_display_name(schema: &str, name: &str, omit_public: bool) -> String {
    if omit_public && schema == "public" {
        name.to_string()
    } else {
        make_qualified_name(schema, name)
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub owner: Option<String>,
    pub columns: Vec<Column>,
    pub primary_key: Option<Vec<String>>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<Index>,
    pub rls: Option<RlsInfo>,
    pub triggers: Vec<Trigger>,
    pub row_count_estimate: Option<i64>,
    pub comment: Option<String>,
}

impl Table {
    pub fn qualified_name(&self) -> String {
        make_qualified_name(&self.schema, &self.name)
    }

    pub fn display_name(&self, omit_public: bool) -> String {
        make_display_name(&self.schema, &self.name, omit_public)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSignature {
    pub schema: String,
    pub name: String,
    pub signature: String,
}

impl TableSignature {
    pub fn qualified_name(&self) -> String {
        make_qualified_name(&self.schema, &self.name)
    }
}

impl TableSummary {
    pub fn new(
        schema: String,
        name: String,
        row_count_estimate: Option<i64>,
        has_rls: bool,
    ) -> Self {
        let qualified_name_lower = make_qualified_name(&schema, &name).to_lowercase();
        Self {
            schema,
            name,
            row_count_estimate,
            has_rls,
            qualified_name_lower,
        }
    }

    pub fn qualified_name(&self) -> String {
        make_qualified_name(&self.schema, &self.name)
    }

    pub fn qualified_name_lower(&self) -> &str {
        &self.qualified_name_lower
    }

    pub fn display_name(&self, omit_public: bool) -> String {
        make_display_name(&self.schema, &self.name, omit_public)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_table(schema: &str, name: &str) -> Table {
        Table {
            schema: schema.to_string(),
            name: name.to_string(),
            owner: None,
            columns: Vec::new(),
            primary_key: None,
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
        }
    }

    fn make_summary(schema: &str, name: &str) -> TableSummary {
        TableSummary::new(schema.to_string(), name.to_string(), None, false)
    }

    mod qualified_name {
        use super::*;

        #[test]
        fn returns_schema_dot_name() {
            let table = make_table("public", "users");

            assert_eq!(table.qualified_name(), "public.users");
        }
    }

    mod display_name {
        use super::*;

        #[test]
        fn omit_public_true_returns_name_only() {
            let table = make_table("public", "users");

            assert_eq!(table.display_name(true), "users");
        }

        #[test]
        fn omit_public_false_returns_qualified() {
            let table = make_table("public", "users");

            assert_eq!(table.display_name(false), "public.users");
        }
    }

    mod summary {
        use super::*;

        #[test]
        fn display_name_omits_public() {
            let summary = make_summary("public", "orders");

            assert_eq!(summary.display_name(true), "orders");
        }

        #[test]
        fn display_name_keeps_non_public_schema() {
            let summary = make_summary("audit", "logs");

            assert_eq!(summary.display_name(true), "audit.logs");
        }

        #[test]
        fn qualified_name_lower_returns_lowercased() {
            let summary = make_summary("MySchema", "MyTable");

            assert_eq!(summary.qualified_name_lower(), "myschema.mytable");
        }
    }
}
