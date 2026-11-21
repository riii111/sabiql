use std::fmt::Write as _;

use super::{Dialect, LexicalFamily, StatementSplitError};
use crate::sql_lex::is_comment_only;
use crate::sqlite_sql::{build_sqlite_explain_query_plan_sql, split_sqlite_statements};

pub struct Sqlite;

impl Dialect for Sqlite {
    fn quote_identifier(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn quote_text(&self, value: &str) -> String {
        if value.contains('\0') {
            format!("CAST({} AS TEXT)", self.blob_literal(value.as_bytes()))
        } else {
            format!("'{}'", value.replace('\'', "''"))
        }
    }

    fn blob_literal(&self, bytes: &[u8]) -> String {
        let hex = bytes
            .iter()
            .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
                let _ = write!(hex, "{byte:02X}");
                hex
            });
        format!("X'{hex}'")
    }

    fn table_reference(&self, _schema: &str, table: &str) -> String {
        self.quote_identifier(table)
    }

    fn explain_sql(&self, query: &str) -> Option<String> {
        build_sqlite_explain_query_plan_sql(query)
    }

    fn explain_analyze_sql(&self, _query: &str) -> Option<String> {
        None
    }

    fn split_statements(&self, sql: &str) -> Result<Vec<String>, StatementSplitError> {
        Ok(split_sqlite_statements(sql)
            .statements()
            .iter()
            .filter(|statement| !is_comment_only(statement))
            .map(|statement| (*statement).to_string())
            .collect())
    }

    fn lexical_family(&self) -> LexicalFamily {
        LexicalFamily::PostgreSql
    }
}
