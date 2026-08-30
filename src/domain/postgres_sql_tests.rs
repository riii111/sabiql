use crate::QueryValue;

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
            &QueryValue::text("C:\\Users\\test"),
            &[("id".into(), QueryValue::text("1"))],
        );

        assert_eq!(
            sql,
            "UPDATE \"public\".\"users\"\nSET \"path\" = 'C:\\Users\\test'\nWHERE \"id\" = '1';"
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
    fn bulk_delete_rejects_empty_rows() {
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
