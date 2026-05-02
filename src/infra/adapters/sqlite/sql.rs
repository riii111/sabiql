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

    #[test]
    fn row_count_query_quotes_table_name() {
        assert_eq!(
            row_count_query(r#"my"table"#),
            r#"SELECT COUNT(*) AS count FROM "my""table""#
        );
    }

    #[test]
    fn pragma_queries_quote_identifiers() {
        assert_eq!(
            table_xinfo_query(r#"my"table"#),
            r#"PRAGMA table_xinfo("my""table")"#
        );
        assert_eq!(
            table_info_query(r#"my"table"#),
            r#"PRAGMA table_info("my""table")"#
        );
        assert_eq!(
            index_list_query(r#"my"table"#),
            r#"PRAGMA index_list("my""table")"#
        );
        assert_eq!(
            foreign_key_list_query(r#"my"table"#),
            r#"PRAGMA foreign_key_list("my""table")"#
        );
        assert_eq!(
            index_info_query(r#"my"index"#),
            r#"PRAGMA index_info("my""index")"#
        );
    }
}
