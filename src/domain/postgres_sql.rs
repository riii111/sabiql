use std::fmt::Write as _;

use crate::QueryValue;

pub fn split_statements(sql: &str) -> Vec<&str> {
    let bytes = sql.as_bytes();
    let mut statements = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut depth = 0;

    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"') => i = skip_quoted(bytes, i, quote),
            b'$' => i = skip_dollar_quoted(sql, bytes, i),
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                i = skip_line_comment(bytes, i);
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i = skip_block_comment(bytes, i);
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            b';' if depth == 0 => {
                push_statement(sql, start, i, &mut statements);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }

    push_statement(sql, start, sql.len(), &mut statements);
    statements
}

pub fn has_select_into(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut i = 0;
    let mut depth = 0;
    let mut found_from = false;

    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"') => i = skip_quoted(bytes, i, quote),
            b'$' => i = skip_dollar_quoted(lower, bytes, i),
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                i = skip_line_comment(bytes, i);
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i = skip_block_comment(bytes, i);
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            _ if depth == 0 && bytes[i].is_ascii_alphabetic() => {
                let word_start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                match &lower[word_start..i] {
                    "from" => found_from = true,
                    "into" if !found_from => {
                        if word_start >= 6 && lower[..word_start].trim_end().ends_with("insert") {
                            continue;
                        }
                        return true;
                    }
                    _ => {}
                }
            }
            _ => i += 1,
        }
    }

    false
}

fn push_statement<'sql>(sql: &'sql str, start: usize, end: usize, statements: &mut Vec<&'sql str>) {
    let statement = sql[start..end].trim();
    if !statement.is_empty() && !is_comment_only(statement) {
        statements.push(statement);
    }
}

fn is_comment_only(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            i = skip_line_comment(bytes, i);
        } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i = skip_block_comment(bytes, i);
        } else {
            return false;
        }
    }

    true
}

fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            i += 1;
            if i < bytes.len() && bytes[i] == quote {
                i += 1;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_dollar_quoted(sql: &str, bytes: &[u8], mut i: usize) -> usize {
    let tag_start = i;
    i += 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'$' {
        return i;
    }

    let tag = &sql[tag_start..=i];
    i += 1;
    while i + tag.len() <= bytes.len() {
        if sql.as_bytes()[i..].starts_with(tag.as_bytes()) {
            return i + tag.len();
        }
        i += 1;
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block_comment(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
    }
    if i + 1 < bytes.len() {
        i += 2;
    }
    i
}

pub fn build_explain_sql(query: &str) -> Option<String> {
    Some(format!("EXPLAIN {query}"))
}

pub fn build_explain_analyze_sql(query: &str) -> Option<String> {
    Some(format!("EXPLAIN ANALYZE {query}"))
}

pub fn build_update_sql(
    schema: &str,
    table: &str,
    column: &str,
    new_value: &QueryValue,
    pk_pairs: &[(String, QueryValue)],
) -> String {
    let where_clause = row_predicate(pk_pairs);

    format!(
        "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
        quote_ident(schema),
        quote_ident(table),
        quote_ident(column),
        sql_literal(new_value),
        where_clause
    )
}

pub fn build_bulk_delete_sql(
    schema: &str,
    table: &str,
    pk_pairs_per_row: &[Vec<(String, QueryValue)>],
) -> String {
    assert!(
        !pk_pairs_per_row.is_empty(),
        "pk_pairs_per_row must not be empty"
    );

    let predicates = pk_pairs_per_row
        .iter()
        .map(|pk_pairs| row_predicate(pk_pairs))
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
        "DELETE FROM {}.{}\nWHERE {};",
        quote_ident(schema),
        quote_ident(table),
        where_clause
    )
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('\"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "''");
    if value.contains('\\') {
        format!("E'{escaped}'")
    } else {
        format!("'{escaped}'")
    }
}

fn sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => quote_literal(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                let _ = write!(hex, "{byte:02x}");
            }
            format!("'\\x{hex}'")
        }
    }
}

fn equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = quote_ident(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
        _ => format!("{column} = {}", sql_literal(value)),
    }
}

fn row_predicate(pk_pairs: &[(String, QueryValue)]) -> String {
    pk_pairs
        .iter()
        .map(|(column, value)| equality_predicate(column, value))
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
mod tests {
    use crate::QueryValue;

    mod statement_scanner {
        use crate::postgres_sql::split_statements;
        use rstest::rstest;

        #[rstest]
        #[case::two_statements("SELECT 1; SELECT 2", vec!["SELECT 1", "SELECT 2"])]
        #[case::single_statement("SELECT 1", vec!["SELECT 1"])]
        #[case::trailing_semicolon("SELECT 1;", vec!["SELECT 1"])]
        #[case::multi_statement_txn(
            "BEGIN; CREATE TABLE t AS SELECT 1; COMMIT",
            vec!["BEGIN", "CREATE TABLE t AS SELECT 1", "COMMIT"]
        )]
        #[case::escaped_single_quote(
            "SELECT 'it''s'; SELECT 2",
            vec!["SELECT 'it''s'", "SELECT 2"]
        )]
        #[case::escaped_double_quote(
            "SELECT \"a\"\"b\"; SELECT 2",
            vec!["SELECT \"a\"\"b\"", "SELECT 2"]
        )]
        fn valid_input_returns_split_statements(#[case] sql: &str, #[case] expected: Vec<&str>) {
            assert_eq!(split_statements(sql), expected);
        }

        #[rstest]
        #[case::single_quotes("SELECT 'a;b'", vec!["SELECT 'a;b'"])]
        #[case::double_quotes(r#"SELECT "a;b""#, vec![r#"SELECT "a;b""#])]
        #[case::dollar_quote("SELECT $$a;b$$", vec!["SELECT $$a;b$$"])]
        #[case::tagged_dollar_quote("SELECT $tag$a;b$tag$", vec!["SELECT $tag$a;b$tag$"])]
        #[case::line_comment("SELECT 1 -- comment; here\n", vec!["SELECT 1 -- comment; here"])]
        #[case::block_comment("SELECT /* ; */ 1", vec!["SELECT /* ; */ 1"])]
        #[case::unterminated_single_quote("SELECT 'a;b", vec!["SELECT 'a;b"])]
        #[case::unterminated_double_quote("SELECT \"a;b", vec!["SELECT \"a;b"])]
        #[case::unterminated_dollar_quote("SELECT $$a;b", vec!["SELECT $$a;b"])]
        #[case::unicode_before_boundary("SELECT 'İ'; SELECT 2", vec!["SELECT 'İ'", "SELECT 2"])]
        fn quoted_semicolon_returns_single_statement(
            #[case] sql: &str,
            #[case] expected: Vec<&str>,
        ) {
            assert_eq!(split_statements(sql), expected);
        }

        #[test]
        fn dollar_quote_tag_is_case_sensitive() {
            let sql = "SELECT $Tag$a;b$tag$; SELECT 2";

            assert_eq!(split_statements(sql), vec![sql]);
        }

        #[rstest]
        #[case::empty("")]
        #[case::whitespace_only("   ")]
        #[case::comment_only("-- comment\n/* comment */")]
        fn blank_or_comment_only_input_returns_empty(#[case] sql: &str) {
            assert!(split_statements(sql).is_empty());
        }
    }

    mod update {
        use super::*;
        use crate::postgres_sql::build_update_sql;

        #[test]
        fn single_pk_returns_escaped_sql() {
            let sql = build_update_sql(
                "public",
                "users",
                "name",
                &QueryValue::text("O'Reilly"),
                &[("id".into(), QueryValue::text("42"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
            );
        }

        #[test]
        fn composite_pk_returns_where_with_all_keys() {
            let sql = build_update_sql(
                "s",
                "t",
                "name",
                &QueryValue::text("new"),
                &[
                    ("id".into(), QueryValue::text("1")),
                    ("tenant_id".into(), QueryValue::text("7")),
                ],
            );

            assert_eq!(
                sql,
                "UPDATE \"s\".\"t\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }
    }

    mod postgres_explain {
        use crate::postgres_sql::{build_explain_analyze_sql, build_explain_sql};

        #[test]
        fn explain_sql_uses_postgres_prefix() {
            assert_eq!(
                build_explain_sql("SELECT 1"),
                Some("EXPLAIN SELECT 1".to_string())
            );
        }

        #[test]
        fn explain_analyze_sql_uses_postgres_prefix() {
            assert_eq!(
                build_explain_analyze_sql("SELECT 1"),
                Some("EXPLAIN ANALYZE SELECT 1".to_string())
            );
        }
    }

    mod update_edge_cases {
        use super::*;
        use crate::postgres_sql::build_update_sql;

        #[test]
        fn null_value_generates_unquoted_null() {
            let sql = build_update_sql(
                "public",
                "users",
                "name",
                &QueryValue::Null,
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = NULL\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn text_null_value_generates_quoted_text() {
            let sql = build_update_sql(
                "public",
                "users",
                "name",
                &QueryValue::text("NULL"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'NULL'\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn empty_string_value_generates_empty_literal() {
            let sql = build_update_sql(
                "public",
                "users",
                "name",
                &QueryValue::text(""),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = ''\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn build_update_sql_escapes_column_name() {
            let sql = build_update_sql(
                "public",
                "users",
                "my\"col",
                &QueryValue::text("val"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"my\"\"col\" = 'val'\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn backslash_in_value_is_preserved_as_literal() {
            let sql = build_update_sql(
                "public",
                "users",
                "path",
                &QueryValue::text("C:\\Users\\O'Reilly"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"path\" = E'C:\\\\Users\\\\O''Reilly'\nWHERE \"id\" = '1';"
            );
        }
    }

    mod bulk_delete {
        use super::*;
        use crate::postgres_sql::{build_bulk_delete_sql, build_update_sql};

        #[test]
        fn single_pk_single_row_returns_predicate() {
            let rows = vec![vec![("id".to_string(), QueryValue::text("1"))]];

            let sql = build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"users\"\nWHERE \"id\" = '1';");
        }

        #[test]
        fn single_pk_multiple_rows_returns_or_predicates() {
            let rows = vec![
                vec![("id".to_string(), QueryValue::text("1"))],
                vec![("id".to_string(), QueryValue::text("2"))],
                vec![("id".to_string(), QueryValue::text("3"))],
            ];

            let sql = build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE (\"id\" = '1') OR (\"id\" = '2') OR (\"id\" = '3');"
            );
        }

        #[test]
        fn composite_pk_returns_or_predicates() {
            let rows = vec![
                vec![
                    ("id".to_string(), QueryValue::text("1")),
                    ("tenant_id".to_string(), QueryValue::text("a")),
                ],
                vec![
                    ("id".to_string(), QueryValue::text("2")),
                    ("tenant_id".to_string(), QueryValue::text("b")),
                ],
            ];

            let sql = build_bulk_delete_sql("s", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"s\".\"t\"\nWHERE (\"id\" = '1' AND \"tenant_id\" = 'a') OR (\"id\" = '2' AND \"tenant_id\" = 'b');"
            );
        }

        #[test]
        fn null_pk_value_uses_is_null_predicate() {
            let rows = vec![vec![("id".to_string(), QueryValue::Null)]];

            let sql = build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IS NULL;");
        }

        #[test]
        fn update_null_pk_value_uses_is_null_predicate() {
            let sql = build_update_sql(
                "public",
                "users",
                "name",
                &QueryValue::text("new"),
                &[("id".into(), QueryValue::Null)],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'new'\nWHERE \"id\" IS NULL;"
            );
        }

        #[test]
        fn pk_value_with_quotes_is_escaped() {
            let rows = vec![vec![("id".to_string(), QueryValue::text("O'Reilly"))]];

            let sql = build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"id\" = 'O''Reilly';"
            );
        }

        #[test]
        fn empty_string_pk_value_returns_empty_literal() {
            let rows = vec![vec![("id".to_string(), QueryValue::text(""))]];

            let sql = build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" = '';");
        }

        #[test]
        fn build_bulk_delete_sql_escapes_column_name() {
            let rows = vec![vec![("my\"pk".to_string(), QueryValue::text("1"))]];

            let sql = build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"my\"\"pk\" = '1';"
            );
        }

        #[test]
        #[should_panic(expected = "pk_pairs_per_row must not be empty")]
        fn rejects_empty_rows() {
            let _ = build_bulk_delete_sql("public", "t", &[]);
        }
    }

    mod sql_literal_tests {
        use crate::QueryValue;
        use crate::postgres_sql::sql_literal;
        use rstest::rstest;

        #[rstest]
        #[case("NULL", "'NULL'")]
        #[case("null", "'null'")]
        #[case("", "''")]
        #[case("hello", "'hello'")]
        #[case("it's", "'it''s'")]
        #[case("NULL ", "'NULL '")]
        fn formats_sql_literal(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(sql_literal(&QueryValue::text(input)), expected);
        }

        #[test]
        fn formats_non_text_query_values() {
            assert_eq!(sql_literal(&QueryValue::Null), "NULL");
            assert_eq!(
                sql_literal(&QueryValue::Blob(vec![0, 255, 65])),
                "'\\x00ff41'"
            );
            assert_eq!(sql_literal(&QueryValue::SqlLiteral("42".to_string())), "42");
        }
    }
}
