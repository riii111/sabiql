use std::fmt::Write as _;

use super::{Dialect, EngineCapabilities, LexicalFamily, StatementSplitError};
use crate::mysql_sql::{mysql_explain_rejection_message, split_mysql_statements};

pub struct MySql;

impl Dialect for MySql {
    fn quote_identifier(&self, name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }

    fn quote_text(&self, value: &str) -> String {
        let mut escaped = String::with_capacity(value.len());
        for character in value.chars() {
            match character {
                '\0' => escaped.push_str("\\0"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                '\u{0008}' => escaped.push_str("\\b"),
                '\u{001a}' => escaped.push_str("\\Z"),
                '\\' => escaped.push_str("\\\\"),
                '\'' => escaped.push_str("\\'"),
                _ => escaped.push(character),
            }
        }
        format!("'{escaped}'")
    }

    fn blob_literal(&self, bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            let _ = write!(hex, "{byte:02X}");
        }
        format!("X'{hex}'")
    }

    fn table_reference(&self, schema: &str, table: &str) -> String {
        format!(
            "{}.{}",
            self.quote_identifier(schema),
            self.quote_identifier(table)
        )
    }

    fn explain_sql(&self, query: &str) -> Option<String> {
        if mysql_explain_rejection_message(query).is_some() {
            return None;
        }
        Some(format!("EXPLAIN FORMAT=TREE {query}"))
    }

    fn explain_analyze_sql(&self, query: &str) -> Option<String> {
        Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))
    }

    fn split_statements(&self, sql: &str) -> Result<Vec<String>, StatementSplitError> {
        split_mysql_statements(sql).map_err(|error| StatementSplitError(error.0))
    }

    fn lexical_family(&self) -> LexicalFamily {
        LexicalFamily::MySql
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            explain_analyze: true,
            row_level_security: false,
        }
    }
}
