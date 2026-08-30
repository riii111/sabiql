use std::fmt::Write as _;

use crate::QueryValue;

pub fn build_update_sql(
    table: &str,
    column: &str,
    new_value: &QueryValue,
    pk_pairs: &[(String, QueryValue)],
) -> String {
    assert!(!pk_pairs.is_empty(), "pk_pairs must not be empty");
    let where_clause = pk_pairs
        .iter()
        .map(|(column, value)| equality_predicate(column, value))
        .collect::<Vec<_>>()
        .join(" AND ");

    format!(
        "UPDATE {}\nSET {} = {}\nWHERE {};",
        quote_ident(table),
        quote_ident(column),
        sql_literal(new_value),
        where_clause
    )
}

pub fn build_bulk_delete_sql(
    table: &str,
    pk_pairs_per_row: &[Vec<(String, QueryValue)>],
) -> String {
    assert!(
        !pk_pairs_per_row.is_empty() && pk_pairs_per_row.iter().all(|pairs| !pairs.is_empty()),
        "pk_pairs_per_row must contain nonempty identity pairs"
    );

    let predicates = pk_pairs_per_row
        .iter()
        .map(|pairs| {
            pairs
                .iter()
                .map(|(column, value)| equality_predicate(column, value))
                .collect::<Vec<_>>()
                .join(" AND ")
        })
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
        quote_ident(table),
        where_clause
    )
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('\"', "\"\""))
}

fn sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => text_sql_literal(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => blob_sql_literal(bytes),
    }
}

fn equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = quote_ident(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
        _ => format!("{column} = {}", sql_literal(value)),
    }
}

fn blob_sql_literal(bytes: &[u8]) -> String {
    format!("X'{}'", encode_bytes_as_sql_hex(bytes))
}

fn text_sql_literal(value: &str) -> String {
    if value.contains('\0') {
        format!(
            "CAST(X'{}' AS TEXT)",
            encode_bytes_as_sql_hex(value.as_bytes())
        )
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

fn encode_bytes_as_sql_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02X}");
            hex
        })
}

#[cfg(test)]
mod tests {
    mod update_sql {
        use crate::QueryValue;
        use crate::sqlite_sql::build_update_sql;

        #[test]
        fn single_pk_omits_schema_and_escapes_sql() {
            let sql = build_update_sql(
                "users",
                "name",
                &QueryValue::text("O'Reilly"),
                &[("id".into(), QueryValue::text("42"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
            );
        }

        #[test]
        fn composite_pk_returns_where_with_all_keys() {
            let sql = build_update_sql(
                "users",
                "name",
                &QueryValue::text("new"),
                &[
                    ("id".into(), QueryValue::text("1")),
                    ("tenant_id".into(), QueryValue::text("7")),
                ],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }

        #[test]
        fn null_value_generates_unquoted_null() {
            let sql = build_update_sql(
                "users",
                "name",
                &QueryValue::Null,
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = NULL\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn text_null_value_generates_quoted_text() {
            let sql = build_update_sql(
                "users",
                "name",
                &QueryValue::text("NULL"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'NULL'\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn nul_text_value_uses_cast_literal() {
            let sql = build_update_sql(
                "users",
                "name",
                &QueryValue::text("a\0b"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = CAST(X'610062' AS TEXT)\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        #[should_panic(expected = "pk_pairs must not be empty")]
        fn update_rejects_empty_identity_pairs() {
            let _ = build_update_sql("users", "name", &QueryValue::text("new"), &[]);
        }
    }

    mod bulk_delete_sql {
        use crate::QueryValue;
        use crate::sqlite_sql::{build_bulk_delete_sql, build_update_sql};

        #[test]
        fn single_pk_multiple_rows_returns_or_predicates() {
            let rows = vec![
                vec![("id".to_string(), QueryValue::text("1"))],
                vec![("id".to_string(), QueryValue::text("2"))],
            ];

            let sql = build_bulk_delete_sql("users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE (\"id\" = '1') OR (\"id\" = '2');"
            );
        }

        #[test]
        fn composite_pk_returns_or_predicates() {
            let rows = vec![
                vec![
                    ("id".to_string(), QueryValue::text("1")),
                    ("tenant_id".to_string(), QueryValue::text("10")),
                ],
                vec![
                    ("id".to_string(), QueryValue::text("2")),
                    ("tenant_id".to_string(), QueryValue::text("20")),
                ],
            ];

            let sql = build_bulk_delete_sql("users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE (\"id\" = '1' AND \"tenant_id\" = '10') OR (\"id\" = '2' AND \"tenant_id\" = '20');"
            );
        }

        #[test]
        fn update_null_predicate_uses_is_null() {
            let sql = build_update_sql(
                "users",
                "name",
                &QueryValue::text("new"),
                &[("id".into(), QueryValue::Null)],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'new'\nWHERE \"id\" IS NULL;"
            );
        }

        #[test]
        fn null_predicate_uses_is_null() {
            let rows = vec![vec![("id".to_string(), QueryValue::Null)]];

            let sql = build_bulk_delete_sql("users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" IS NULL;");
        }

        #[test]
        fn composite_null_predicate_uses_is_null() {
            let rows = vec![vec![
                ("id".to_string(), QueryValue::Null),
                ("tenant_id".to_string(), QueryValue::text("10")),
            ]];

            let sql = build_bulk_delete_sql("users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE \"id\" IS NULL AND \"tenant_id\" = '10';"
            );
        }

        #[test]
        fn blob_pk_value_uses_blob_literal() {
            let rows = vec![vec![("id".to_string(), QueryValue::Blob(vec![0, 255, 65]))]];

            let sql = build_bulk_delete_sql("users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" = X'00FF41';");
        }

        #[test]
        fn nul_text_pk_value_uses_cast_literal() {
            let rows = vec![vec![(
                "id".to_string(),
                QueryValue::Text("a\0bc".to_string()),
            )]];

            let sql = build_bulk_delete_sql("users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE \"id\" = CAST(X'61006263' AS TEXT);"
            );
        }

        #[test]
        #[should_panic(expected = "pk_pairs_per_row must contain nonempty identity pairs")]
        fn bulk_delete_rejects_empty_rows() {
            let _ = build_bulk_delete_sql("users", &[]);
        }

        #[test]
        #[should_panic(expected = "pk_pairs_per_row must contain nonempty identity pairs")]
        fn bulk_delete_rejects_empty_identity_pairs() {
            let _ = build_bulk_delete_sql("users", &[vec![]]);
        }
    }
}
