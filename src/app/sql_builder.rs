use crate::domain::{DatabaseType, QueryValue, mysql_sql, postgres_sql, sqlite_sql};

pub fn build_explain_sql(database_type: DatabaseType, query: &str) -> Option<String> {
    match database_type {
        DatabaseType::PostgreSQL => postgres_sql::build_explain_sql(query),
        DatabaseType::MySQL => mysql_sql::build_explain_sql(query),
        DatabaseType::SQLite => sqlite_sql::build_sqlite_explain_query_plan_sql(query),
    }
}

pub fn build_explain_analyze_sql(database_type: DatabaseType, query: &str) -> Option<String> {
    match database_type {
        DatabaseType::PostgreSQL => postgres_sql::build_explain_analyze_sql(query),
        DatabaseType::MySQL => mysql_sql::build_explain_analyze_sql(query),
        DatabaseType::SQLite => None,
    }
}

pub fn build_update_sql(
    database_type: DatabaseType,
    schema: &str,
    table: &str,
    column: &str,
    new_value: &QueryValue,
    pk_pairs: &[(String, QueryValue)],
) -> String {
    match database_type {
        DatabaseType::PostgreSQL => {
            postgres_sql::build_update_sql(schema, table, column, new_value, pk_pairs)
        }
        DatabaseType::MySQL => {
            mysql_sql::build_update_sql(schema, table, column, new_value, pk_pairs)
        }
        DatabaseType::SQLite => sqlite_sql::build_update_sql(table, column, new_value, pk_pairs),
    }
}

pub fn build_bulk_delete_sql(
    database_type: DatabaseType,
    schema: &str,
    table: &str,
    pk_pairs_per_row: &[Vec<(String, QueryValue)>],
) -> String {
    match database_type {
        DatabaseType::PostgreSQL => {
            postgres_sql::build_bulk_delete_sql(schema, table, pk_pairs_per_row)
        }
        DatabaseType::MySQL => mysql_sql::build_bulk_delete_sql(schema, table, pk_pairs_per_row),
        DatabaseType::SQLite => sqlite_sql::build_bulk_delete_sql(table, pk_pairs_per_row),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_explain_sql_to_each_database() {
        assert_eq!(
            build_explain_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN FORMAT=TREE SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_sql(DatabaseType::SQLite, "SELECT 1"),
            Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
        );
    }

    #[test]
    fn dispatches_explain_analyze_sql_to_supported_databases() {
        assert_eq!(
            build_explain_analyze_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_analyze_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE FORMAT=TREE SELECT 1".to_string())
        );
        assert_eq!(
            build_explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"),
            None
        );
    }

    #[test]
    fn dispatches_update_sql_with_database_specific_output() {
        let pairs = [("id".to_string(), QueryValue::Null)];

        assert_eq!(
            build_update_sql(
                DatabaseType::PostgreSQL,
                "public",
                "users",
                "name",
                &QueryValue::text("Ada"),
                &pairs,
            ),
            "UPDATE \"public\".\"users\"\nSET \"name\" = 'Ada'\nWHERE \"id\" IS NULL;"
        );
        assert_eq!(
            build_update_sql(
                DatabaseType::MySQL,
                "sabiql_test",
                "users",
                "name",
                &QueryValue::text("Ada"),
                &pairs,
            ),
            "UPDATE `sabiql_test`.`users`\nSET `name` = 'Ada'\nWHERE `id` IS NULL;"
        );
        assert_eq!(
            build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("a\0b"),
                &pairs,
            ),
            "UPDATE \"users\"\nSET \"name\" = CAST(X'610062' AS TEXT)\nWHERE \"id\" IS NULL;"
        );
    }

    #[test]
    fn dispatches_bulk_delete_sql_with_database_specific_output() {
        let rows = vec![
            vec![("id".to_string(), QueryValue::SqlLiteral("1".into()))],
            vec![("id".to_string(), QueryValue::SqlLiteral("2".into()))],
        ];

        assert_eq!(
            build_bulk_delete_sql(DatabaseType::PostgreSQL, "public", "users", &rows),
            "DELETE FROM \"public\".\"users\"\nWHERE (\"id\" = 1) OR (\"id\" = 2);"
        );
        assert_eq!(
            build_bulk_delete_sql(DatabaseType::MySQL, "sabiql_test", "users", &rows),
            "DELETE FROM `sabiql_test`.`users`\nWHERE (`id` = 1) OR (`id` = 2);"
        );
        assert_eq!(
            build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows),
            "DELETE FROM \"users\"\nWHERE (\"id\" = 1) OR (\"id\" = 2);"
        );
    }

    fn update_sql(
        engine: DatabaseType,
        schema: &str,
        table: &str,
        column: &str,
        value: &QueryValue,
        identity: &[(String, QueryValue)],
    ) -> String {
        build_update_sql(engine, schema, table, column, value, identity)
    }

    fn bulk_delete_sql(
        engine: DatabaseType,
        schema: &str,
        table: &str,
        rows: &[Vec<(String, QueryValue)>],
    ) -> String {
        build_bulk_delete_sql(engine, schema, table, rows)
    }

    fn explain_sql(engine: DatabaseType, query: &str) -> Option<String> {
        build_explain_sql(engine, query)
    }

    fn explain_analyze_sql(engine: DatabaseType, query: &str) -> Option<String> {
        build_explain_analyze_sql(engine, query)
    }

    fn table_reference(engine: DatabaseType, schema: &str, table: &str) -> String {
        rendered_update_parts(engine, schema, table, "c", &QueryValue::Null)[0].clone()
    }

    fn quote_identifier(engine: DatabaseType, name: &str) -> String {
        rendered_update_parts(engine, "s", "t", name, &QueryValue::Null)[1].clone()
    }

    fn value_literal(engine: DatabaseType, value: &QueryValue) -> String {
        rendered_update_parts(engine, "s", "t", "c", value)[2].clone()
    }

    fn rendered_update_parts(
        engine: DatabaseType,
        schema: &str,
        table: &str,
        column: &str,
        value: &QueryValue,
    ) -> [String; 3] {
        // Quoting is private to each engine's statement builder, so it is read back from an UPDATE.
        let sql = update_sql(
            engine,
            schema,
            table,
            column,
            value,
            &[("id".to_string(), QueryValue::SqlLiteral("1".to_string()))],
        );
        let (target, rest) = sql
            .strip_prefix("UPDATE ")
            .and_then(|rest| rest.split_once("\nSET "))
            .expect("UPDATE starts with its target table");
        let (assignment, _) = rest
            .rsplit_once("\nWHERE ")
            .expect("UPDATE ends with its row predicate");
        let (column, literal) = assignment
            .split_once(" = ")
            .expect("UPDATE assigns one column");
        [target, column, literal].map(str::to_string)
    }

    mod quoting {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case::plain("users", "\"users\"")]
        #[case::double_quote("a\"b", "\"a\"\"b\"")]
        #[case::backtick("a`b", "\"a`b\"")]
        #[case::empty("", "\"\"")]
        fn postgres_and_sqlite_identifiers_double_embedded_double_quotes(
            #[case] name: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(quote_identifier(DatabaseType::PostgreSQL, name), expected);
            assert_eq!(quote_identifier(DatabaseType::SQLite, name), expected);
        }

        #[rstest]
        #[case::plain("users", "`users`")]
        #[case::double_quote("a\"b", "`a\"b`")]
        #[case::backtick("a`b", "`a``b`")]
        #[case::empty("", "``")]
        fn mysql_identifiers_double_embedded_backticks(#[case] name: &str, #[case] expected: &str) {
            assert_eq!(quote_identifier(DatabaseType::MySQL, name), expected);
        }

        #[rstest]
        #[case::postgres(DatabaseType::PostgreSQL, "\"app\".\"users\"")]
        #[case::mysql(DatabaseType::MySQL, "`app`.`users`")]
        #[case::sqlite(DatabaseType::SQLite, "\"users\"")]
        fn table_references_qualify_by_schema_except_sqlite(
            #[case] engine: DatabaseType,
            #[case] expected: &str,
        ) {
            assert_eq!(table_reference(engine, "app", "users"), expected);
        }

        #[rstest]
        #[case::backslash("C:\\temp", "E'C:\\\\temp'")]
        #[case::backslash_before_quote("a\\'b", "E'a\\\\''b'")]
        #[case::lone_backslash("\\", "E'\\\\'")]
        fn postgres_text_with_backslashes_uses_escape_string_syntax(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(
                value_literal(DatabaseType::PostgreSQL, &QueryValue::text(value)),
                expected
            );
        }

        #[rstest]
        #[case::single_quote("O'Reilly", "'O''Reilly'")]
        #[case::nul("a\0b", "'a\0b'")]
        #[case::newline("a\nb", "'a\nb'")]
        #[case::tab("a\tb", "'a\tb'")]
        #[case::substitute("a\u{1a}b", "'a\u{1a}b'")]
        #[case::non_ascii("日本語", "'日本語'")]
        fn postgres_text_without_backslashes_only_doubles_single_quotes(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(
                value_literal(DatabaseType::PostgreSQL, &QueryValue::text(value)),
                expected
            );
        }

        #[rstest]
        #[case::single_quote("O'Reilly", "'O\\'Reilly'")]
        #[case::backslash("C:\\temp", "'C:\\\\temp'")]
        #[case::non_ascii("日本語", "'日本語'")]
        fn mysql_text_backslash_escapes_quotes_and_backslashes(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(
                value_literal(DatabaseType::MySQL, &QueryValue::text(value)),
                expected
            );
        }

        #[rstest]
        #[case::nul("a\0b", "'a\\0b'")]
        #[case::newline("a\nb", "'a\\nb'")]
        #[case::carriage_return("a\rb", "'a\\rb'")]
        #[case::tab("a\tb", "'a\\tb'")]
        #[case::backspace("a\u{8}b", "'a\\bb'")]
        #[case::substitute("a\u{1a}b", "'a\\Zb'")]
        fn mysql_text_backslash_escapes_control_characters(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(
                value_literal(DatabaseType::MySQL, &QueryValue::text(value)),
                expected
            );
        }

        #[rstest]
        #[case::single_quote("O'Reilly", "'O''Reilly'")]
        #[case::backslash("C:\\temp", "'C:\\temp'")]
        #[case::newline("a\nb", "'a\nb'")]
        #[case::tab("a\tb", "'a\tb'")]
        #[case::substitute("a\u{1a}b", "'a\u{1a}b'")]
        #[case::non_ascii("日本語", "'日本語'")]
        fn sqlite_text_without_nul_only_doubles_single_quotes(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(
                value_literal(DatabaseType::SQLite, &QueryValue::text(value)),
                expected
            );
        }

        #[rstest]
        #[case::ascii("a\0b", "CAST(X'610062' AS TEXT)")]
        #[case::non_ascii("é\0", "CAST(X'C3A900' AS TEXT)")]
        fn sqlite_text_with_nul_is_cast_from_utf8_hex(#[case] value: &str, #[case] expected: &str) {
            assert_eq!(
                value_literal(DatabaseType::SQLite, &QueryValue::text(value)),
                expected
            );
        }

        #[rstest]
        #[case::postgres(DatabaseType::PostgreSQL, "'\\x00ff41'", "'\\x'")]
        #[case::mysql(DatabaseType::MySQL, "X'00FF41'", "X''")]
        #[case::sqlite(DatabaseType::SQLite, "X'00FF41'", "X''")]
        fn binary_values_use_engine_hex_literals(
            #[case] engine: DatabaseType,
            #[case] expected: &str,
            #[case] expected_empty: &str,
        ) {
            assert_eq!(
                value_literal(engine, &QueryValue::Blob(vec![0x00, 0xff, 0x41])),
                expected
            );
            assert_eq!(
                value_literal(engine, &QueryValue::Blob(Vec::new())),
                expected_empty
            );
        }

        #[rstest]
        #[case::postgres(DatabaseType::PostgreSQL)]
        #[case::mysql(DatabaseType::MySQL)]
        #[case::sqlite(DatabaseType::SQLite)]
        fn null_and_sql_literals_are_rendered_verbatim(#[case] engine: DatabaseType) {
            assert_eq!(value_literal(engine, &QueryValue::Null), "NULL");
            assert_eq!(
                value_literal(engine, &QueryValue::SqlLiteral("42".to_string())),
                "42"
            );
        }
    }

    mod write_statements {
        use super::*;
        use rstest::rstest;

        fn mixed_identity() -> Vec<(String, QueryValue)> {
            vec![
                ("id".to_string(), QueryValue::SqlLiteral("1".to_string())),
                ("tenant".to_string(), QueryValue::Null),
                ("key".to_string(), QueryValue::Blob(vec![0xab])),
            ]
        }

        fn mixed_rows() -> Vec<Vec<(String, QueryValue)>> {
            vec![
                vec![
                    ("id".to_string(), QueryValue::SqlLiteral("1".to_string())),
                    ("tenant".to_string(), QueryValue::text("a\0b")),
                ],
                vec![
                    ("id".to_string(), QueryValue::SqlLiteral("2".to_string())),
                    ("tenant".to_string(), QueryValue::Null),
                ],
                vec![
                    ("id".to_string(), QueryValue::Blob(vec![0x01])),
                    ("tenant".to_string(), QueryValue::text("O'Reilly")),
                ],
            ]
        }

        #[rstest]
        #[case::postgres(
            DatabaseType::PostgreSQL,
            "UPDATE \"app\".\"users\"\nSET \"name\" = 'a\0b'\nWHERE \"id\" = 1 AND \"tenant\" IS NULL AND \"key\" = '\\xab';"
        )]
        #[case::mysql(
            DatabaseType::MySQL,
            "UPDATE `app`.`users`\nSET `name` = 'a\\0b'\nWHERE `id` = 1 AND `tenant` IS NULL AND `key` = X'AB';"
        )]
        #[case::sqlite(
            DatabaseType::SQLite,
            "UPDATE \"users\"\nSET \"name\" = CAST(X'610062' AS TEXT)\nWHERE \"id\" = 1 AND \"tenant\" IS NULL AND \"key\" = X'AB';"
        )]
        fn update_matches_every_identity_column(
            #[case] engine: DatabaseType,
            #[case] expected: &str,
        ) {
            assert_eq!(
                update_sql(
                    engine,
                    "app",
                    "users",
                    "name",
                    &QueryValue::text("a\0b"),
                    &mixed_identity(),
                ),
                expected
            );
        }

        #[rstest]
        #[case::postgres(
            DatabaseType::PostgreSQL,
            "DELETE FROM \"app\".\"users\"\nWHERE (\"id\" = 1 AND \"tenant\" = 'a\0b') OR (\"id\" = 2 AND \"tenant\" IS NULL) OR (\"id\" = '\\x01' AND \"tenant\" = 'O''Reilly');"
        )]
        #[case::mysql(
            DatabaseType::MySQL,
            "DELETE FROM `app`.`users`\nWHERE (`id` = 1 AND `tenant` = 'a\\0b') OR (`id` = 2 AND `tenant` IS NULL) OR (`id` = X'01' AND `tenant` = 'O\\'Reilly');"
        )]
        #[case::sqlite(
            DatabaseType::SQLite,
            "DELETE FROM \"users\"\nWHERE (\"id\" = 1 AND \"tenant\" = CAST(X'610062' AS TEXT)) OR (\"id\" = 2 AND \"tenant\" IS NULL) OR (\"id\" = X'01' AND \"tenant\" = 'O''Reilly');"
        )]
        fn bulk_delete_joins_parenthesized_row_identities_with_or(
            #[case] engine: DatabaseType,
            #[case] expected: &str,
        ) {
            assert_eq!(
                bulk_delete_sql(engine, "app", "users", &mixed_rows()),
                expected
            );
        }

        #[rstest]
        #[case::postgres(
            DatabaseType::PostgreSQL,
            "DELETE FROM \"app\".\"users\"\nWHERE \"id\" = 1;"
        )]
        #[case::mysql(DatabaseType::MySQL, "DELETE FROM `app`.`users`\nWHERE `id` = 1;")]
        #[case::sqlite(DatabaseType::SQLite, "DELETE FROM \"users\"\nWHERE \"id\" = 1;")]
        fn bulk_delete_of_one_row_leaves_its_identity_unparenthesized(
            #[case] engine: DatabaseType,
            #[case] expected: &str,
        ) {
            let rows = vec![vec![(
                "id".to_string(),
                QueryValue::SqlLiteral("1".to_string()),
            )]];

            assert_eq!(bulk_delete_sql(engine, "app", "users", &rows), expected);
        }
    }

    mod explain {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case::select("SELECT 1")]
        #[case::ddl("CREATE TABLE t (id int)")]
        #[case::multiple_statements("SELECT 1; SELECT 2")]
        fn postgres_prefixes_any_input(#[case] query: &str) {
            assert_eq!(
                explain_sql(DatabaseType::PostgreSQL, query),
                Some(format!("EXPLAIN {query}"))
            );
            assert_eq!(
                explain_analyze_sql(DatabaseType::PostgreSQL, query),
                Some(format!("EXPLAIN ANALYZE {query}"))
            );
        }

        #[rstest]
        #[case::select("SELECT 1", Some("EXPLAIN FORMAT=TREE SELECT 1"))]
        #[case::update(
            "UPDATE users SET active = TRUE",
            Some("EXPLAIN FORMAT=TREE UPDATE users SET active = TRUE")
        )]
        #[case::ddl("CREATE TABLE t (id INT)", None)]
        #[case::client_command("\\C /tmp/other.sock", None)]
        #[case::multiple_statements("SELECT 1; SELECT 2", None)]
        fn mysql_uses_tree_format_only_for_supported_statements(
            #[case] query: &str,
            #[case] expected: Option<&str>,
        ) {
            assert_eq!(explain_sql(DatabaseType::MySQL, query).as_deref(), expected);
        }

        #[rstest]
        #[case::select("SELECT 1")]
        #[case::update("UPDATE users SET active = TRUE")]
        #[case::ddl("CREATE TABLE t (id INT)")]
        fn mysql_analyze_prefixes_without_checking_the_statement(#[case] query: &str) {
            assert_eq!(
                explain_analyze_sql(DatabaseType::MySQL, query),
                Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))
            );
        }

        #[rstest]
        #[case::select("SELECT 1", Some("EXPLAIN QUERY PLAN SELECT 1"))]
        #[case::delete("DELETE FROM users", Some("EXPLAIN QUERY PLAN DELETE FROM users"))]
        #[case::query_plan_input(
            "EXPLAIN QUERY PLAN SELECT 1",
            Some("EXPLAIN QUERY PLAN SELECT 1")
        )]
        #[case::explain_input("EXPLAIN SELECT 1", None)]
        #[case::ddl("CREATE TABLE t (id INTEGER)", None)]
        #[case::multiple_statements("SELECT 1; SELECT 2", None)]
        fn sqlite_uses_query_plan_only_for_supported_statements(
            #[case] query: &str,
            #[case] expected: Option<&str>,
        ) {
            assert_eq!(
                explain_sql(DatabaseType::SQLite, query).as_deref(),
                expected
            );
        }

        #[test]
        fn sqlite_analyze_is_unavailable() {
            assert_eq!(explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"), None);
        }
    }
}
