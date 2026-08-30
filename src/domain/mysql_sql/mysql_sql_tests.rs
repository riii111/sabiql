mod explain {
    use crate::mysql_sql::{build_explain_analyze_sql, build_explain_sql};

    #[test]
    fn builds_tree_explain_for_supported_queries() {
        for query in [
            "SELECT * FROM users",
            "TABLE users",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "REPLACE users VALUES (1)",
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
            "SELECT * FROM users FOR UPDATE",
        ] {
            assert_eq!(
                build_explain_sql(query),
                Some(format!("EXPLAIN FORMAT=TREE {query}")),
                "{query}"
            );
        }
    }

    #[test]
    fn rejects_tree_explain_for_unsupported_input() {
        for query in [
            "CREATE TABLE users(id INT)",
            "DROP TABLE users",
            "REPLACE",
            "REPLACE INTO",
            "\\C /tmp/other.sock",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(build_explain_sql(query), None, "{query}");
        }
    }

    #[test]
    fn builds_tree_explain_analyze_only_for_side_effect_free_reads() {
        for query in ["SELECT * FROM users", "TABLE users"] {
            assert_eq!(
                build_explain_analyze_sql(query),
                Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}")),
                "{query}"
            );
        }

        for query in [
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "SELECT * FROM users FOR UPDATE",
            "SELECT `GET_LOCK`('sabiql', 0)",
            "SELECT `RELEASE_LOCK`('sabiql')",
            "SELECT `RELEASE_ALL_LOCKS`()",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(build_explain_analyze_sql(query), None, "{query}");
        }
    }
}

mod write {
    use crate::QueryValue;
    use crate::mysql_sql::{build_bulk_delete_sql, build_update_sql};

    #[test]
    fn update_quotes_identifiers_and_mysql_string_escapes() {
        let sql = build_update_sql(
            "db`name",
            "table`name",
            "value`name",
            &QueryValue::text("O'Reilly\\path\n\t\0"),
            &[
                (
                    "id`part".to_string(),
                    QueryValue::SqlLiteral("18446744073709551615".into()),
                ),
                ("tenant".to_string(), QueryValue::Null),
            ],
        );

        assert_eq!(
            sql,
            "UPDATE \x60db\x60\x60name\x60.\x60table\x60\x60name\x60\nSET \x60value\x60\x60name\x60 = 'O\\'Reilly\\\\path\\n\\t\\0'\nWHERE \x60id\x60\x60part\x60 = 18446744073709551615 AND \x60tenant\x60 IS NULL;"
        );
    }

    #[test]
    fn update_uses_text_datetime_and_blob_literals_without_coercion() {
        let sql = build_update_sql(
            "sabiql_test",
            "events",
            "payload",
            &QueryValue::Blob(vec![0, 255, 16]),
            &[(
                "created_at".to_string(),
                QueryValue::text("2026-08-13 12:34:56"),
            )],
        );

        assert_eq!(
            sql,
            "UPDATE `sabiql_test`.`events`\nSET `payload` = X'00FF10'\nWHERE `created_at` = '2026-08-13 12:34:56';"
        );
    }

    #[test]
    fn update_uses_null_literal_for_cell_value() {
        let sql = build_update_sql(
            "sabiql_test",
            "items",
            "payload",
            &QueryValue::Null,
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );

        assert_eq!(
            sql,
            "UPDATE `sabiql_test`.`items`\nSET `payload` = NULL\nWHERE `id` = 1;"
        );
    }

    #[test]
    fn json_document_update_keeps_json_null_distinct_from_string_null() {
        let json_null = build_update_sql(
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text("null"),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );
        let string_null = build_update_sql(
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text(r#""null""#),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );

        assert!(json_null.contains("SET `payload` = 'null'"));
        assert!(string_null.contains("SET `payload` = '\"null\"'"));
        assert_ne!(json_null, string_null);
    }

    #[test]
    fn bulk_delete_targets_each_composite_primary_key_row() {
        let sql = build_bulk_delete_sql(
            "sabiql_test",
            "items",
            &[
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("1".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("20".into())),
                ],
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("2".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("10".into())),
                ],
            ],
        );

        assert_eq!(
            sql,
            "DELETE FROM `sabiql_test`.`items`\nWHERE (`first` = 1 AND `second` = 20) OR (`first` = 2 AND `second` = 10);"
        );
    }

    #[test]
    fn bulk_delete_uses_unwrapped_predicate_for_single_row() {
        let sql = build_bulk_delete_sql(
            "sabiql_test",
            "items",
            &[vec![("id".to_string(), QueryValue::SqlLiteral("1".into()))]],
        );

        assert_eq!(sql, "DELETE FROM `sabiql_test`.`items`\nWHERE `id` = 1;");
    }

    #[test]
    #[should_panic(expected = "pk_pairs_per_row must not be empty")]
    fn bulk_delete_rejects_empty_rows() {
        let _ = build_bulk_delete_sql("sabiql_test", "items", &[]);
    }
}
