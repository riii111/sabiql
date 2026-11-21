mod lexical_family;
mod mysql;
mod postgres;
mod sqlite;

use crate::{DatabaseType, QueryValue};

pub use lexical_family::LexicalFamily;
pub use mysql::MySql;
pub use postgres::Postgres;
pub use sqlite::Sqlite;

pub trait Dialect: Sync {
    fn quote_identifier(&self, name: &str) -> String;
    fn quote_text(&self, value: &str) -> String;
    fn blob_literal(&self, bytes: &[u8]) -> String;
    fn table_reference(&self, schema: &str, table: &str) -> String;
    fn explain_sql(&self, query: &str) -> Option<String>;
    fn explain_analyze_sql(&self, query: &str) -> Option<String>;
    fn split_statements(&self, sql: &str) -> Result<Vec<String>, StatementSplitError>;
    fn lexical_family(&self) -> LexicalFamily;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct StatementSplitError(String);

impl DatabaseType {
    #[deny(
        clippy::wildcard_enum_match_arm,
        clippy::match_wildcard_for_single_variants,
        reason = "Every engine must name its dialect so a new engine fails to compile instead of silently reusing another engine's SQL."
    )]
    pub fn dialect(self) -> &'static dyn Dialect {
        match self {
            Self::PostgreSQL => &Postgres,
            Self::SQLite => &Sqlite,
            Self::MySQL => &MySql,
        }
    }
}

// Engines must not override the statement shape, so the shared builders are inherent methods of the trait object rather than default methods.
impl dyn Dialect {
    pub fn value_literal(&self, value: &QueryValue) -> String {
        match value {
            QueryValue::Null => "NULL".to_string(),
            QueryValue::Text(text) => self.quote_text(text),
            QueryValue::SqlLiteral(literal) => literal.clone(),
            QueryValue::Blob(bytes) => self.blob_literal(bytes),
        }
    }

    pub fn update_sql(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        assert!(!pk_pairs.is_empty(), "pk_pairs must not be empty");

        format!(
            "UPDATE {}\nSET {} = {}\nWHERE {};",
            self.table_reference(schema, table),
            self.quote_identifier(column),
            self.value_literal(new_value),
            self.row_predicate(pk_pairs)
        )
    }

    pub fn bulk_delete_sql(
        &self,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );
        assert!(
            pk_pairs_per_row.iter().all(|pairs| !pairs.is_empty()),
            "pk_pairs_per_row must contain nonempty identity pairs"
        );

        let predicates = pk_pairs_per_row
            .iter()
            .map(|pairs| self.row_predicate(pairs))
            .collect::<Vec<_>>();
        let where_clause = if predicates.len() == 1 {
            predicates[0].clone()
        } else {
            predicates
                .into_iter()
                .map(|predicate| format!("({predicate})"))
                .collect::<Vec<_>>()
                .join(" OR ")
        };

        format!(
            "DELETE FROM {}\nWHERE {};",
            self.table_reference(schema, table),
            where_clause
        )
    }

    fn row_predicate(&self, pk_pairs: &[(String, QueryValue)]) -> String {
        pk_pairs
            .iter()
            .map(|(column, value)| self.equality_predicate(column, value))
            .collect::<Vec<_>>()
            .join(" AND ")
    }

    fn equality_predicate(&self, column: &str, value: &QueryValue) -> String {
        let column = self.quote_identifier(column);
        match value {
            QueryValue::Null => format!("{column} IS NULL"),
            _ => format!("{column} = {}", self.value_literal(value)),
        }
    }
}

#[cfg(test)]
mod tests;
