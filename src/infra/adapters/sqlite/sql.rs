fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn user_tables_query() -> &'static str {
    r"
    SELECT name, sql
    FROM sqlite_schema
    WHERE type = 'table'
      AND name NOT LIKE 'sqlite_%'
    ORDER BY name
    "
}

pub(super) fn row_count_query(table: &str) -> String {
    format!("SELECT COUNT(*) AS count FROM {}", quote_ident(table))
}

pub(super) fn table_xinfo_query(table: &str) -> String {
    format!("PRAGMA table_xinfo({})", quote_ident(table))
}

pub(super) fn table_info_query(table: &str) -> String {
    format!("PRAGMA table_info({})", quote_ident(table))
}

pub(super) fn index_list_query(table: &str) -> String {
    format!("PRAGMA index_list({})", quote_ident(table))
}

pub(super) fn index_info_query(index: &str) -> String {
    format!("PRAGMA index_info({})", quote_ident(index))
}

pub(super) fn foreign_key_list_query(table: &str) -> String {
    format!("PRAGMA foreign_key_list({})", quote_ident(table))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_ident_escapes_embedded_quotes() {
        assert_eq!(quote_ident(r#"my"table"#), r#""my""table""#);
    }
}
