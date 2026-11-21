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

mod engine_selection {
    use super::*;

    #[test]
    fn dispatches_explain_sql_to_each_database() {
        assert_eq!(
            explain_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN SELECT 1".to_string())
        );
        assert_eq!(
            explain_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN FORMAT=TREE SELECT 1".to_string())
        );
        assert_eq!(
            explain_sql(DatabaseType::SQLite, "SELECT 1"),
            Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
        );
    }

    #[test]
    fn dispatches_explain_analyze_sql_to_supported_databases() {
        assert_eq!(
            explain_analyze_sql(DatabaseType::PostgreSQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE SELECT 1".to_string())
        );
        assert_eq!(
            explain_analyze_sql(DatabaseType::MySQL, "SELECT 1"),
            Some("EXPLAIN ANALYZE FORMAT=TREE SELECT 1".to_string())
        );
        assert_eq!(explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"), None);
    }

    #[test]
    fn dispatches_update_sql_with_database_specific_output() {
        let pairs = [("id".to_string(), QueryValue::Null)];

        assert_eq!(
            update_sql(
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
            update_sql(
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
            update_sql(
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
            bulk_delete_sql(DatabaseType::PostgreSQL, "public", "users", &rows),
            "DELETE FROM \"public\".\"users\"\nWHERE (\"id\" = 1) OR (\"id\" = 2);"
        );
        assert_eq!(
            bulk_delete_sql(DatabaseType::MySQL, "sabiql_test", "users", &rows),
            "DELETE FROM `sabiql_test`.`users`\nWHERE (`id` = 1) OR (`id` = 2);"
        );
        assert_eq!(
            bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows),
            "DELETE FROM \"users\"\nWHERE (\"id\" = 1) OR (\"id\" = 2);"
        );
    }
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

    #[rstest]
    #[case::postgres(DatabaseType::PostgreSQL)]
    #[case::mysql(DatabaseType::MySQL)]
    #[case::sqlite(DatabaseType::SQLite)]
    #[should_panic(expected = "pk_pairs must not be empty")]
    fn update_rejects_empty_identity_pairs(#[case] engine: DatabaseType) {
        let _ = update_sql(
            engine,
            "app",
            "users",
            "name",
            &QueryValue::text("new"),
            &[],
        );
    }

    #[rstest]
    #[case::postgres(DatabaseType::PostgreSQL)]
    #[case::mysql(DatabaseType::MySQL)]
    #[case::sqlite(DatabaseType::SQLite)]
    #[should_panic(expected = "pk_pairs_per_row must not be empty")]
    fn bulk_delete_rejects_empty_rows(#[case] engine: DatabaseType) {
        let _ = bulk_delete_sql(engine, "app", "users", &[]);
    }

    #[rstest]
    #[case::postgres(DatabaseType::PostgreSQL)]
    #[case::mysql(DatabaseType::MySQL)]
    #[case::sqlite(DatabaseType::SQLite)]
    #[should_panic(expected = "pk_pairs_per_row must contain nonempty identity pairs")]
    fn bulk_delete_rejects_empty_identity_pairs(#[case] engine: DatabaseType) {
        let _ = bulk_delete_sql(engine, "app", "users", &[vec![]]);
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
    fn sqlite_analyze_is_unavailable() {
        assert_eq!(explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"), None);
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
    #[case::single("SELECT 1", vec!["SELECT 1"])]
    #[case::two("SELECT 1; SELECT 2", vec!["SELECT 1", "SELECT 2"])]
    #[case::trailing_semicolon("SELECT 1;", vec!["SELECT 1"])]
    #[case::empty("", Vec::<&str>::new())]
    #[case::whitespace_only("   ", Vec::<&str>::new())]
    fn basic_split(#[case] sql: &str, #[case] expected: Vec<&str>) {
        assert_eq!(split_statements(sql), expected);
    }

    #[rstest]
    #[case::single_quote("SELECT 'a;b'", vec!["SELECT 'a;b'"])]
    #[case::double_quote("SELECT \"a;b\"", vec!["SELECT \"a;b\""])]
    #[case::dollar_quote("SELECT $$a;b$$", vec!["SELECT $$a;b$$"])]
    #[case::tagged_dollar_quote("SELECT $tag$a;b$tag$", vec!["SELECT $tag$a;b$tag$"])]
    fn semicolon_in_strings(#[case] sql: &str, #[case] expected: Vec<&str>) {
        assert_eq!(split_statements(sql), expected);
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

    #[rstest]
    #[case::line_comment("SELECT 1 -- ;comment\n; SELECT 2", vec!["SELECT 1 -- ;comment", "SELECT 2"])]
    #[case::block_comment("SELECT /* ; */ 1; SELECT 2", vec!["SELECT /* ; */ 1", "SELECT 2"])]
    fn semicolon_in_comments(#[case] sql: &str, #[case] expected: Vec<&str>) {
        assert_eq!(split_statements(sql), expected);
    }

    #[test]
    fn do_block_split() {
        let sql = "DO $$ BEGIN RAISE NOTICE 'hi'; END $$; SELECT 1";
        let result = split_statements(sql);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "DO $$ BEGIN RAISE NOTICE 'hi'; END $$");
        assert_eq!(result[1], "SELECT 1");
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

    #[test]
    fn comment_only_input() {
        let sql = "-- just a comment";
        let result = split_statements(sql);
        assert!(result.is_empty());
    }

    #[test]
    fn unclosed_quote() {
        let sql = "SELECT 'unclosed";
        let result = split_statements(sql);
        assert_eq!(result, vec!["SELECT 'unclosed"]);
    }

    #[test]
    fn non_ascii_before_semicolon() {
        // Case-folding of İ (U+0130) changes byte length in lowercase.
        // Byte offsets must come from the original sql, not the lowercased copy.
        let sql = "SELECT 'İ'; SELECT 2";
        let result = split_statements(sql);
        assert_eq!(result, vec!["SELECT 'İ'", "SELECT 2"]);
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

        #[rstest]
        #[case::quoted_literal("SELECT 'a; SELECT 2", "unterminated MySQL quoted literal")]
        #[case::block_comment("SELECT 1 /* note", "unterminated MySQL block comment")]
        #[case::parentheses("SELECT (1; SELECT 2", "unbalanced MySQL parentheses")]
        fn mysql_lex_failures_return_the_lexer_message(#[case] sql: &str, #[case] message: &str) {
            assert_eq!(
                split_for(DatabaseType::MySQL, sql),
                Err(message.to_string())
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

        #[test]
        fn sqlite_keeps_trigger_body_statements_together() {
            assert_eq!(
                split_for(
                    DatabaseType::SQLite,
                    "CREATE TRIGGER audit AFTER INSERT ON users BEGIN INSERT INTO log VALUES (1); END; SELECT 1"
                ),
                Ok(owned(&[
                    "CREATE TRIGGER audit AFTER INSERT ON users BEGIN INSERT INTO log VALUES (1); END",
                    "SELECT 1",
                ]))
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

mod postgres {
    use crate::{DatabaseType, QueryValue};

    fn build_update_sql(
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        DatabaseType::PostgreSQL
            .dialect()
            .update_sql(schema, table, column, new_value, pk_pairs)
    }

    fn build_bulk_delete_sql(
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        DatabaseType::PostgreSQL
            .dialect()
            .bulk_delete_sql(schema, table, pk_pairs_per_row)
    }

    fn build_explain_sql(query: &str) -> Option<String> {
        DatabaseType::PostgreSQL.dialect().explain_sql(query)
    }

    fn build_explain_analyze_sql(query: &str) -> Option<String> {
        DatabaseType::PostgreSQL
            .dialect()
            .explain_analyze_sql(query)
    }

    fn sql_literal(value: &QueryValue) -> String {
        DatabaseType::PostgreSQL.dialect().value_literal(value)
    }

    fn quote_ident(value: &str) -> String {
        DatabaseType::PostgreSQL.dialect().quote_identifier(value)
    }

    fn quote_literal(value: &str) -> String {
        DatabaseType::PostgreSQL.dialect().quote_text(value)
    }

    mod update {
        use super::*;

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
        use super::{build_explain_analyze_sql, build_explain_sql};

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
    }

    mod sql_literal_tests {
        use super::*;
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

    mod quoting {
        use super::*;
        use rstest::rstest;

        #[test]
        fn quote_ident_escapes_embedded_quotes() {
            assert_eq!(quote_ident(r#"user"name"#), r#""user""name""#);
        }

        #[test]
        fn quote_ident_supports_empty_string() {
            assert_eq!(quote_ident(""), "\"\"");
        }

        #[rstest]
        #[case::backtick("a`b", "\"a`b\"")]
        #[case::backslash("a\\b", "\"a\\b\"")]
        #[case::non_ascii("日本語", "\"日本語\"")]
        fn quote_ident_keeps_other_characters_verbatim(#[case] name: &str, #[case] expected: &str) {
            assert_eq!(quote_ident(name), expected);
        }

        #[rstest]
        #[case::nul("a\0b", "'a\0b'")]
        #[case::newline("a\nb", "'a\nb'")]
        #[case::tab("a\tb", "'a\tb'")]
        #[case::substitute("a\u{1a}b", "'a\u{1a}b'")]
        #[case::non_ascii("日本語", "'日本語'")]
        fn quote_literal_keeps_characters_other_than_quotes_verbatim(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(quote_literal(value), expected);
        }

        #[test]
        fn quote_literal_escapes_embedded_quotes() {
            assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
        }

        #[test]
        fn quote_literal_supports_only_quotes() {
            assert_eq!(quote_literal("''"), "''''''");
        }

        #[test]
        fn quote_literal_supports_empty_string() {
            assert_eq!(quote_literal(""), "''");
        }
    }
}

mod mysql {
    use crate::{DatabaseType, QueryValue};

    fn build_explain_sql(query: &str) -> Option<String> {
        DatabaseType::MySQL.dialect().explain_sql(query)
    }

    fn build_explain_analyze_sql(query: &str) -> Option<String> {
        DatabaseType::MySQL.dialect().explain_analyze_sql(query)
    }

    fn build_update_sql(
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        DatabaseType::MySQL
            .dialect()
            .update_sql(schema, table, column, new_value, pk_pairs)
    }

    fn build_bulk_delete_sql(
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        DatabaseType::MySQL
            .dialect()
            .bulk_delete_sql(schema, table, pk_pairs_per_row)
    }

    fn quote_identifier(value: &str) -> String {
        DatabaseType::MySQL.dialect().quote_identifier(value)
    }

    fn quote_string(value: &str) -> String {
        DatabaseType::MySQL.dialect().quote_text(value)
    }

    mod explain {
        use super::{build_explain_analyze_sql, build_explain_sql};

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
        fn builds_tree_explain_analyze_for_supported_queries() {
            for query in ["SELECT * FROM users", "TABLE users"] {
                assert_eq!(
                    build_explain_analyze_sql(query),
                    Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}")),
                    "{query}"
                );
            }
        }
    }

    mod write {
        use super::{QueryValue, build_bulk_delete_sql, build_update_sql};

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
    }

    mod quoting {
        use super::{quote_identifier, quote_string};
        use rstest::rstest;

        #[rstest]
        #[case::plain("users", "`users`")]
        #[case::double_quote("a\"b", "`a\"b`")]
        #[case::backtick("a`b", "`a``b`")]
        #[case::empty("", "``")]
        fn identifiers_double_embedded_backticks(#[case] name: &str, #[case] expected: &str) {
            assert_eq!(quote_identifier(name), expected);
        }

        #[rstest]
        #[case::single_quote("O'Reilly", "'O\\'Reilly'")]
        #[case::backslash("C:\\temp", "'C:\\\\temp'")]
        #[case::non_ascii("日本語", "'日本語'")]
        fn strings_backslash_escape_quotes_and_backslashes(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(quote_string(value), expected);
        }

        #[rstest]
        #[case::nul("a\0b", "'a\\0b'")]
        #[case::newline("a\nb", "'a\\nb'")]
        #[case::carriage_return("a\rb", "'a\\rb'")]
        #[case::tab("a\tb", "'a\\tb'")]
        #[case::backspace("a\u{8}b", "'a\\bb'")]
        #[case::substitute("a\u{1a}b", "'a\\Zb'")]
        fn strings_backslash_escape_control_characters(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(quote_string(value), expected);
        }
    }
}

mod sqlite {
    use crate::{DatabaseType, QueryValue};

    fn build_update_sql(
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        DatabaseType::SQLite
            .dialect()
            .update_sql("main", table, column, new_value, pk_pairs)
    }

    fn build_bulk_delete_sql(
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        DatabaseType::SQLite
            .dialect()
            .bulk_delete_sql("main", table, pk_pairs_per_row)
    }

    fn quote_ident(value: &str) -> String {
        DatabaseType::SQLite.dialect().quote_identifier(value)
    }

    fn quote_literal(value: &str) -> String {
        DatabaseType::SQLite.dialect().quote_text(value)
    }

    mod update_sql {
        use super::{QueryValue, build_update_sql};

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
    }

    mod bulk_delete_sql {
        use super::{QueryValue, build_bulk_delete_sql, build_update_sql};

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
    }

    mod quoting {
        use super::*;
        use rstest::rstest;

        #[test]
        fn identifier_escapes_embedded_quotes() {
            assert_eq!(quote_ident(r#"my"table"#), r#""my""table""#);
        }

        #[test]
        fn literal_escapes_embedded_quotes() {
            assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
        }

        #[rstest]
        #[case::backtick("a`b", "\"a`b\"")]
        #[case::empty("", "\"\"")]
        #[case::non_ascii("日本語", "\"日本語\"")]
        fn identifier_keeps_other_characters_verbatim(#[case] name: &str, #[case] expected: &str) {
            assert_eq!(quote_ident(name), expected);
        }

        #[rstest]
        #[case::backslash("C:\\temp", "'C:\\temp'")]
        #[case::newline("a\nb", "'a\nb'")]
        #[case::tab("a\tb", "'a\tb'")]
        #[case::substitute("a\u{1a}b", "'a\u{1a}b'")]
        #[case::non_ascii("日本語", "'日本語'")]
        fn literal_keeps_characters_other_than_quotes_verbatim(
            #[case] value: &str,
            #[case] expected: &str,
        ) {
            assert_eq!(quote_literal(value), expected);
        }
    }
}
