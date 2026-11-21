use std::fmt::Write as _;

use super::{Dialect, StatementSplitError};
use crate::postgres_sql;

pub struct Postgres;

impl Dialect for Postgres {
    fn quote_identifier(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn quote_text(&self, value: &str) -> String {
        let escaped = value.replace('\\', "\\\\").replace('\'', "''");
        if value.contains('\\') {
            format!("E'{escaped}'")
        } else {
            format!("'{escaped}'")
        }
    }

    fn blob_literal(&self, bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            let _ = write!(hex, "{byte:02x}");
        }
        format!("'\\x{hex}'")
    }

    fn table_reference(&self, schema: &str, table: &str) -> String {
        format!(
            "{}.{}",
            self.quote_identifier(schema),
            self.quote_identifier(table)
        )
    }

    fn explain_sql(&self, query: &str) -> Option<String> {
        Some(format!("EXPLAIN {query}"))
    }

    fn explain_analyze_sql(&self, query: &str) -> Option<String> {
        Some(format!("EXPLAIN ANALYZE {query}"))
    }

    fn split_statements(&self, sql: &str) -> Result<Vec<String>, StatementSplitError> {
        Ok(postgres_sql::split_statements(sql)
            .into_iter()
            .map(str::to_owned)
            .collect())
    }
}
