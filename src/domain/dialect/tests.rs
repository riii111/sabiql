use crate::{DatabaseType, QueryValue};

fn update_sql(
    engine: DatabaseType,
    schema: &str,
    table: &str,
    column: &str,
    value: &QueryValue,
    identity: &[(String, QueryValue)],
) -> String {
    engine
        .dialect()
        .update_sql(schema, table, column, value, identity)
}

fn bulk_delete_sql(
    engine: DatabaseType,
    schema: &str,
    table: &str,
    rows: &[Vec<(String, QueryValue)>],
) -> String {
    engine.dialect().bulk_delete_sql(schema, table, rows)
}

fn explain_sql(engine: DatabaseType, query: &str) -> Option<String> {
    engine.dialect().explain_sql(query)
}

fn explain_analyze_sql(engine: DatabaseType, query: &str) -> Option<String> {
    engine.dialect().explain_analyze_sql(query)
}

fn table_reference(engine: DatabaseType, schema: &str, table: &str) -> String {
    engine.dialect().table_reference(schema, table)
}

fn quote_identifier(engine: DatabaseType, name: &str) -> String {
    engine.dialect().quote_identifier(name)
}

fn value_literal(engine: DatabaseType, value: &QueryValue) -> String {
    engine.dialect().value_literal(value)
}

mod quoting {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::plain("users", "\"users\"")]
    #[case::double_quote("a\"b", "\"a\"\"b\"")]
    #[case::backtick("a`b", "\"a`b\"")]
    #[case::backslash("a\\b", "\"a\\b\"")]
    #[case::non_ascii("日本語", "\"日本語\"")]
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
    #[case::only_quotes("''", "''''''")]
    #[case::nul("a\0b", "'a\0b'")]
    #[case::newline("a\nb", "'a\nb'")]
    #[case::tab("a\tb", "'a\tb'")]
    #[case::substitute("a\u{1a}b", "'a\u{1a}b'")]
    #[case::non_ascii("日本語", "'日本語'")]
    #[case::trailing_space("NULL ", "'NULL '")]
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

    #[test]
    fn null_and_sql_literals_are_rendered_verbatim() {
        assert_eq!(
            value_literal(DatabaseType::PostgreSQL, &QueryValue::Null),
            "NULL"
        );
        assert_eq!(
            value_literal(
                DatabaseType::PostgreSQL,
                &QueryValue::SqlLiteral("42".to_string())
            ),
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
    fn update_matches_every_identity_column(#[case] engine: DatabaseType, #[case] expected: &str) {
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
    #[case::sql_null(DatabaseType::PostgreSQL, QueryValue::Null, "SET \"name\" = NULL")]
    #[case::text_null(
        DatabaseType::PostgreSQL,
        QueryValue::text("NULL"),
        "SET \"name\" = 'NULL'"
    )]
    #[case::empty_text(DatabaseType::PostgreSQL, QueryValue::text(""), "SET \"name\" = ''")]
    #[case::postgres_backslash_and_quote(
        DatabaseType::PostgreSQL,
        QueryValue::text("C:\\Users\\O'Reilly"),
        "SET \"name\" = E'C:\\\\Users\\\\O''Reilly'"
    )]
    #[case::mysql_blob(
        DatabaseType::MySQL,
        QueryValue::Blob(vec![0, 255, 16]),
        "SET `name` = X'00FF10'"
    )]
    #[case::mysql_datetime_text(
        DatabaseType::MySQL,
        QueryValue::text("2026-08-13 12:34:56"),
        "SET `name` = '2026-08-13 12:34:56'"
    )]
    fn update_sets_each_value_kind_without_coercion(
        #[case] engine: DatabaseType,
        #[case] value: QueryValue,
        #[case] expected_set: &str,
    ) {
        let identity = [("id".to_string(), QueryValue::SqlLiteral("1".to_string()))];

        let sql = update_sql(engine, "app", "users", "name", &value, &identity);

        assert_eq!(sql.lines().nth(1), Some(expected_set));
    }

    #[test]
    fn update_quotes_identifiers_and_mysql_string_escapes() {
        let sql = update_sql(
            DatabaseType::MySQL,
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
    fn json_document_update_keeps_json_null_distinct_from_string_null() {
        let json_null = update_sql(
            DatabaseType::MySQL,
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text("null"),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );
        let string_null = update_sql(
            DatabaseType::MySQL,
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
        "DELETE FROM \"app\".\"users\"\nWHERE \"my\"\"pk\" = '';"
    )]
    #[case::mysql(DatabaseType::MySQL, "DELETE FROM `app`.`users`\nWHERE `my\"pk` = '';")]
    #[case::sqlite(
        DatabaseType::SQLite,
        "DELETE FROM \"users\"\nWHERE \"my\"\"pk\" = '';"
    )]
    fn bulk_delete_of_one_row_leaves_its_identity_unparenthesized(
        #[case] engine: DatabaseType,
        #[case] expected: &str,
    ) {
        let rows = vec![vec![("my\"pk".to_string(), QueryValue::text(""))]];

        assert_eq!(bulk_delete_sql(engine, "app", "users", &rows), expected);
    }

    #[test]
    #[should_panic(expected = "pk_pairs must not be empty")]
    fn update_rejects_empty_identity_pairs() {
        let _ = update_sql(
            DatabaseType::PostgreSQL,
            "app",
            "users",
            "name",
            &QueryValue::text("new"),
            &[],
        );
    }

    #[test]
    #[should_panic(expected = "pk_pairs_per_row must not be empty")]
    fn bulk_delete_rejects_empty_rows() {
        let _ = bulk_delete_sql(DatabaseType::PostgreSQL, "app", "users", &[]);
    }

    #[test]
    #[should_panic(expected = "pk_pairs_per_row must contain nonempty identity pairs")]
    fn bulk_delete_rejects_empty_identity_pairs() {
        let _ = bulk_delete_sql(DatabaseType::PostgreSQL, "app", "users", &[vec![]]);
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
    #[case::select("SELECT 1")]
    #[case::table("TABLE users")]
    #[case::insert("INSERT INTO users VALUES (1)")]
    #[case::replace_into("REPLACE INTO users VALUES (1)")]
    #[case::replace_without_into("REPLACE users VALUES (1)")]
    #[case::update("UPDATE users SET active = TRUE")]
    #[case::delete("DELETE FROM users WHERE id = 1")]
    #[case::locking_read("SELECT * FROM users FOR UPDATE")]
    fn mysql_uses_tree_format_for_supported_statements(#[case] query: &str) {
        assert_eq!(
            explain_sql(DatabaseType::MySQL, query),
            Some(format!("EXPLAIN FORMAT=TREE {query}"))
        );
    }

    #[rstest]
    #[case::ddl("CREATE TABLE t (id INT)")]
    #[case::replace_without_target("REPLACE")]
    #[case::replace_into_without_target("REPLACE INTO")]
    #[case::client_command("\\C /tmp/other.sock")]
    #[case::multiple_statements("SELECT 1; SELECT 2")]
    fn mysql_rejects_tree_format_for_unsupported_input(#[case] query: &str) {
        assert_eq!(explain_sql(DatabaseType::MySQL, query), None);
    }

    #[rstest]
    #[case::select("SELECT 1")]
    #[case::table("TABLE users")]
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
    #[case::query_plan_input("EXPLAIN QUERY PLAN SELECT 1", Some("EXPLAIN QUERY PLAN SELECT 1"))]
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
    fn analyze_capability_matches_analyze_sql_availability() {
        for engine in DatabaseType::all() {
            let dialect = engine.dialect();

            assert_eq!(
                dialect.capabilities().explain_analyze,
                dialect.explain_analyze_sql("SELECT 1").is_some(),
                "{engine}"
            );
        }
    }
}

mod split_statements_tests {
    use super::*;
    use rstest::rstest;

    fn split_statements(sql: &str) -> Vec<String> {
        DatabaseType::PostgreSQL
            .dialect()
            .split_statements(sql)
            .expect("PostgreSQL statement splitting is infallible")
    }

    #[rstest]
    #[case::bracket_quote(
        "DROP TABLE [a;b]; SELECT 1",
        vec!["DROP TABLE [a", "b]", "SELECT 1"]
    )]
    #[case::backtick_quote(
        "DROP TABLE `a;b`; SELECT 1",
        vec!["DROP TABLE `a", "b`", "SELECT 1"]
    )]
    #[case::bracket_contains_drop(
        "SELECT [1; DROP TABLE users]",
        vec!["SELECT [1", "DROP TABLE users]"]
    )]
    #[case::backtick_contains_drop(
        "SELECT `1; DROP TABLE users`",
        vec!["SELECT `1", "DROP TABLE users`"]
    )]
    fn postgres_brackets_and_backticks_do_not_hide_semicolons(
        #[case] sql: &str,
        #[case] expected: Vec<&str>,
    ) {
        assert_eq!(split_statements(sql), expected);
    }

    #[test]
    fn escaped_quote_no_split() {
        let sql = "SELECT 'it''s;here'";
        let result = split_statements(sql);
        assert_eq!(result, vec!["SELECT 'it''s;here'"]);
    }

    #[test]
    fn trailing_comment_only() {
        let sql = "SELECT 1; -- comment";
        let result = split_statements(sql);
        assert_eq!(result, vec!["SELECT 1"]);
    }

    mod by_engine {
        use super::*;

        fn split_for(database_type: DatabaseType, sql: &str) -> Result<Vec<String>, String> {
            database_type
                .dialect()
                .split_statements(sql)
                .map_err(|error| error.to_string())
        }

        fn owned(statements: &[&str]) -> Vec<String> {
            statements.iter().map(ToString::to_string).collect()
        }

        #[test]
        fn postgres_keeps_dollar_quoted_semicolons_in_one_statement() {
            assert_eq!(
                split_for(DatabaseType::PostgreSQL, "SELECT $$a;b$$; SELECT 2"),
                Ok(owned(&["SELECT $$a;b$$", "SELECT 2"]))
            );
        }

        #[rstest]
        #[case::mysql(DatabaseType::MySQL)]
        #[case::sqlite(DatabaseType::SQLite)]
        fn other_engines_split_inside_dollar_quotes(#[case] database_type: DatabaseType) {
            assert_eq!(
                split_for(database_type, "SELECT $$a;b$$"),
                Ok(owned(&["SELECT $$a", "b$$"]))
            );
        }

        #[test]
        fn mysql_lex_failures_return_the_lexer_message() {
            assert_eq!(
                split_for(DatabaseType::MySQL, "SELECT (1; SELECT 2"),
                Err("unbalanced MySQL parentheses".to_string())
            );
        }

        #[test]
        fn mysql_drops_hash_comment_only_fragments() {
            assert_eq!(
                split_for(DatabaseType::MySQL, "SELECT 1; # note\n; SELECT 2"),
                Ok(owned(&["SELECT 1", "SELECT 2"]))
            );
        }

        #[test]
        fn sqlite_drops_comment_only_fragments() {
            assert_eq!(
                split_for(
                    DatabaseType::SQLite,
                    "SELECT 1; -- note\n; /* block */; SELECT 2; -- trailing"
                ),
                Ok(owned(&["SELECT 1", "SELECT 2"]))
            );
        }

        #[rstest]
        #[case::unclosed_body(
            "CREATE TRIGGER audit AFTER INSERT ON users BEGIN INSERT INTO log VALUES (1);"
        )]
        #[case::missing_body("CREATE TRIGGER audit AFTER INSERT ON users")]
        fn sqlite_returns_fragments_despite_trigger_split_errors(#[case] sql: &str) {
            assert_eq!(split_for(DatabaseType::SQLite, sql), Ok(owned(&[sql])));
        }
    }
}

mod lexical_family {
    use super::*;

    fn completion_keywords(engine: DatabaseType) -> Vec<&'static str> {
        engine
            .dialect()
            .lexical_family()
            .completion_keywords()
            .collect()
    }

    #[test]
    fn keyword_inventory_preserves_each_database_set() {
        let expected_postgresql = "SELECT FROM WHERE JOIN LEFT RIGHT INNER OUTER CROSS ON AND OR NOT IN IS NULL TRUE FALSE LIKE ILIKE BETWEEN EXISTS CASE WHEN THEN ELSE END AS DISTINCT ORDER BY ASC DESC NULLS FIRST LAST GROUP HAVING LIMIT OFFSET UNION INTERSECT EXCEPT ALL INSERT INTO VALUES UPDATE SET DELETE CREATE DROP ALTER TABLE INDEX VIEW RETURNING WITH RECURSIVE COALESCE NULLIF CAST USING";
        let expected_mysql = "SELECT FROM WHERE JOIN LEFT RIGHT INNER OUTER CROSS ON AND OR NOT IN IS NULL TRUE FALSE LIKE BETWEEN EXISTS CASE WHEN THEN ELSE END AS DISTINCT ORDER BY ASC DESC GROUP HAVING LIMIT OFFSET UNION INTERSECT EXCEPT ALL INSERT INTO VALUES UPDATE SET DELETE TRUNCATE CREATE DROP ALTER TABLE INDEX VIEW WITH RECURSIVE COALESCE NULLIF CAST USING NATURAL WINDOW OVER PARTITION ROWS RANGE UNBOUNDED PRECEDING FOLLOWING CURRENT ROW EXPLAIN ANALYZE SHOW DESCRIBE DATABASE DATABASES PRIMARY KEY FOREIGN REFERENCES UNIQUE DEFAULT CONSTRAINT CHECK IF CASCADE RENAME MODIFY COLUMN ENGINE CHARACTER CHARSET COLLATE AUTO_INCREMENT FOR LOCK SHARE START TRANSACTION COMMIT ROLLBACK";

        let postgresql = completion_keywords(DatabaseType::PostgreSQL);
        let sqlite = completion_keywords(DatabaseType::SQLite);
        let mut mysql = completion_keywords(DatabaseType::MySQL);
        let mut expected_mysql = expected_mysql.split_whitespace().collect::<Vec<_>>();

        assert_eq!(
            postgresql,
            expected_postgresql.split_whitespace().collect::<Vec<_>>()
        );
        assert_eq!(sqlite, postgresql);
        mysql.sort_unstable();
        expected_mysql.sort_unstable();
        assert_eq!(mysql, expected_mysql);
    }
}
