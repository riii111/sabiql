use crate::adapters::test_support;

use super::*;

mod query_results {
    use super::*;

    #[tokio::test]
    async fn select_returns_query_result() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(&dsn, "SELECT 1 AS value", AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
        assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
    }

    #[tokio::test]
    async fn runs_sql_larger_than_the_windows_command_line_limit() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();
        let sql = format!("SELECT 1 AS value /* {} */", "x".repeat(32_768));

        let result = adapter
            .execute_adhoc(&dsn, &sql, AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
    }

    #[tokio::test]
    async fn long_text_result_does_not_materialize_display_rows() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT replace(hex(zeroblob(2048)), '00', 'xx') AS body",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        assert_eq!(
            result
                .value_at(0, 0)
                .and_then(QueryValue::as_str)
                .map(str::len),
            Some(4096)
        );
    }

    #[tokio::test]
    async fn explain_query_plan_returns_readable_detail_lines() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
                 CREATE INDEX idx_users_name ON users(name);",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "EXPLAIN QUERY PLAN SELECT * FROM users WHERE name = 'alice'",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        let plan_text = sqlite_explain_query_plan_text_from_result(&result).unwrap();

        assert!(!plan_text.trim().is_empty(), "plan text must not be empty");
        assert!(
            plan_text.to_ascii_lowercase().contains("users"),
            "expected users table in plan, got: {plan_text}"
        );
        assert!(
            !explain_plan_operation_lines(&plan_text).is_empty(),
            "expected at least one SCAN/SEARCH operation, got: {plan_text}"
        );
    }

    #[tokio::test]
    async fn explain_query_plan_for_join_includes_both_scan_targets() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            "CREATE TABLE users(id INTEGER PRIMARY KEY);
                 CREATE TABLE orders(id INTEGER, user_id INTEGER);",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "EXPLAIN QUERY PLAN SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        let plan_text = sqlite_explain_query_plan_text_from_result(&result).unwrap();
        let operation_lines = explain_plan_operation_lines(&plan_text);

        assert!(
            operation_lines.len() >= 2,
            "expected multiple plan operations, got: {plan_text}"
        );
        assert!(
            plan_mentions_table_or_alias(&plan_text, "users", 'u'),
            "expected users side in plan, got: {plan_text}"
        );
        assert!(
            plan_mentions_table_or_alias(&plan_text, "orders", 'o'),
            "expected orders side in plan, got: {plan_text}"
        );
    }

    #[tokio::test]
    async fn explain_query_plan_delete_does_not_modify_database() {
        let (_dir, dsn) = test_support::make_sqlite_db(
            "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
                 INSERT INTO users(name) VALUES ('alice'), ('bob');
                 CREATE INDEX idx_users_name ON users(name);",
        );
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "EXPLAIN QUERY PLAN DELETE FROM users WHERE name = 'alice'",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();
        let rows = adapter
            .execute_adhoc(
                &dsn,
                "SELECT COUNT(*) AS total FROM users",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        let plan_text = sqlite_explain_query_plan_text_from_result(&result).unwrap();

        assert!(
            plan_text.to_ascii_lowercase().contains("users"),
            "expected users table in plan, got: {plan_text}"
        );
        assert_eq!(test_support::display_row(&rows, 0), vec!["2".to_string()]);
    }

    fn explain_plan_operation_lines(plan_text: &str) -> Vec<&str> {
        plan_text
            .lines()
            .filter(|line| {
                let upper = line.to_ascii_uppercase();
                upper.contains("SCAN") || upper.contains("SEARCH")
            })
            .collect()
    }

    fn plan_mentions_table_or_alias(plan_text: &str, table: &str, alias: char) -> bool {
        let lower = plan_text.to_ascii_lowercase();
        lower.contains(table)
            || lower
                .split_whitespace()
                .any(|token| token == alias.to_string())
    }
}

mod trigger_execution {
    use super::*;

    #[tokio::test]
    async fn create_trigger_with_multi_statement_body_preserves_definition() {
        let setup = r"
            CREATE TABLE agent_messages(
                id INTEGER PRIMARY KEY,
                role TEXT NOT NULL,
                content TEXT NOT NULL
            );
            CREATE VIRTUAL TABLE agent_messages_fts USING fts5(role, content);
            ";
        let trigger = r"
            CREATE TRIGGER agent_messages_fts_ai AFTER INSERT ON agent_messages BEGIN
                INSERT INTO agent_messages_fts(rowid, role, content)
                VALUES (new.id, new.role, new.content);
            END
            ";
        let (_dir, dsn) = test_support::make_sqlite_db(setup);
        let adapter = SqliteAdapter::new();

        adapter
            .execute_adhoc(&dsn, trigger, AccessMode::ReadWrite)
            .await
            .unwrap();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'agent_messages_fts_ai'", AccessMode::ReadOnly)
            .await
            .unwrap();

        let stored = result.display_value_at(0, 0).unwrap().replace('\n', " ");
        let expected = trigger.trim().replace('\n', " ");
        assert!(
            !stored.contains("__sabiql_sqlite_probe_"),
            "probe SQL must not appear in stored trigger definition: {stored}"
        );
        assert_eq!(stored, expected);
    }

    #[tokio::test]
    async fn create_trigger_referencing_new_end_preserves_definition() {
        let setup = r"
            CREATE TABLE events(
                id INTEGER PRIMARY KEY,
                end INTEGER NOT NULL
            );
            CREATE TABLE counters(
                id INTEGER PRIMARY KEY,
                end_value INTEGER
            );
            CREATE TABLE audit(
                event_id INTEGER,
                end_value INTEGER
            );
            ";
        let trigger = r"
            CREATE TRIGGER sync_end AFTER UPDATE ON events BEGIN
                UPDATE counters SET end_value = new.end WHERE id = new.id;
                INSERT INTO audit(event_id, end_value) VALUES (new.id, new.end);
            END
            ";
        let (_dir, dsn) = test_support::make_sqlite_db(setup);
        let adapter = SqliteAdapter::new();

        adapter
            .execute_adhoc(&dsn, trigger, AccessMode::ReadWrite)
            .await
            .unwrap();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'sync_end'",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        let stored = result.display_value_at(0, 0).unwrap().replace('\n', " ");
        let expected = trigger.trim().replace('\n', " ");
        assert!(
            !stored.contains("__sabiql_sqlite_probe_"),
            "probe SQL must not appear in stored trigger definition: {stored}"
        );
        assert_eq!(stored, expected);
    }

    #[tokio::test]
    async fn unclosed_create_trigger_fails_before_execution() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let error = adapter
            .execute_adhoc(
                &dsn,
                "CREATE TRIGGER t AFTER INSERT ON users BEGIN INSERT INTO logs(id) VALUES (1);",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, DbOperationError::QueryFailed(_)));
    }
}

mod result_set_parsing {
    use super::*;

    #[tokio::test]
    async fn select_preserves_quoted_newline_in_multicolumn_result() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT);");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT 'line 1' || char(10) || 'line 2' AS body, 'ok' AS marker",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["body", "marker"]);
        assert_eq!(
            test_support::display_row(&result, 0),
            vec!["line 1\nline 2".to_string(), "ok".to_string()]
        );
        assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
    }

    #[tokio::test]
    async fn multi_select_preserves_quoted_newline_in_last_result() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
        .execute_adhoc(
            &dsn,
            "SELECT 1 AS ignored; SELECT 'line 1' || char(10) || 'line 2' AS body, 'ok' AS marker", AccessMode::ReadOnly)
        .await
        .unwrap();

        assert_eq!(result.columns, vec!["body", "marker"]);
        assert_eq!(
            test_support::display_row(&result, 0),
            vec!["line 1\nline 2".to_string(), "ok".to_string()]
        );
        assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
    }

    #[tokio::test]
    async fn multi_select_does_not_treat_data_row_as_next_header() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT 1 AS a, 2 AS b UNION ALL SELECT 3, 4; SELECT 5 AS c, 6 AS d",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["c", "d"]);
        assert_eq!(
            test_support::display_row(&result, 0),
            vec!["5".to_string(), "6".to_string()]
        );
        assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
    }

    #[tokio::test]
    async fn multi_select_empty_trailing_result_preserves_projection_columns() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT 1 AS a; SELECT 2 AS b WHERE false",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["b"]);
        assert_eq!(result.data_row_count(), 0);
        assert_eq!(result.command_tag, Some(CommandTag::Select(0)));
    }

    #[tokio::test]
    async fn empty_cte_select_preserves_projection_columns() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(
                &dsn,
                "WITH q(value) AS (VALUES (1)) SELECT value FROM q WHERE false",
                AccessMode::ReadOnly,
            )
            .await
            .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.data_row_count(), 0);
        assert_eq!(result.command_tag, Some(CommandTag::Select(0)));
    }

    #[tokio::test]
    async fn pragma_result_does_not_get_select_command_tag() {
        let (_dir, dsn) =
            test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(&dsn, "PRAGMA table_info(users)", AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(
            result.columns,
            vec!["cid", "name", "type", "notnull", "dflt_value", "pk"]
        );
        assert_eq!(result.command_tag, None);
    }
}

mod session_configuration {
    use super::*;

    fn safe_mode_error(result: Result<QueryResult, DbOperationError>, expected: &[&str]) {
        assert!(matches!(
            result,
            Err(DbOperationError::QueryFailed(details))
                if expected.iter().any(|message| details.contains(message))
        ));
    }

    #[tokio::test]
    async fn enables_foreign_keys_before_user_sql() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(&dsn, "PRAGMA foreign_keys", AccessMode::ReadWrite)
            .await
            .unwrap();

        assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
    }

    #[tokio::test]
    async fn read_only_session_enables_query_only_before_user_sql() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(&dsn, "PRAGMA query_only", AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
    }

    #[tokio::test]
    async fn applies_busy_timeout_before_user_sql() {
        let (_dir, dsn) = test_support::make_sqlite_db("");
        let adapter = SqliteAdapter::new();

        let result = adapter
            .execute_adhoc(&dsn, "PRAGMA busy_timeout", AccessMode::ReadOnly)
            .await
            .unwrap();

        assert_eq!(
            test_support::display_row(&result, 0),
            vec!["5000".to_string()]
        );
    }

    #[rstest::rstest]
    #[case::writefile(
            "writefile",
            &["cannot use the writefile() function in safe mode"],
        )]
    #[case::readfile(
            "readfile",
            &["cannot use the readfile() function in safe mode"],
        )]
    #[case::load_extension(
            "load_extension",
            &[
                "cannot use the load_extension() function in safe mode",
                "no such function: load_extension",
            ],
        )]
    #[case::attach("attach", &["cannot run ATTACH in safe mode"])]
    #[tokio::test]
    async fn safe_mode_rejects_host_side_effects_in_read_write_sessions(
        #[case] side_effect: &str,
        #[case] expected: &[&str],
    ) {
        let (dir, dsn) = test_support::make_sqlite_db("");
        let attached = dir.path().join("attached.db");
        let output = dir.path().join("output.txt");
        std::fs::write(&attached, []).unwrap();
        let adapter = SqliteAdapter::new();

        let sql = match side_effect {
            "writefile" => format!("SELECT writefile('{}', 'hello')", output.display()),
            "readfile" => format!("SELECT readfile('{}')", attached.display()),
            "load_extension" => "SELECT load_extension('/tmp/sabiql-extension')".to_string(),
            "attach" => format!("ATTACH DATABASE '{}' AS attached", attached.display()),
            _ => unreachable!(),
        };
        safe_mode_error(
            adapter
                .execute_adhoc(&dsn, &sql, AccessMode::ReadWrite)
                .await,
            expected,
        );
        if side_effect == "writefile" {
            assert!(!output.exists());
        }
    }

    #[rstest::rstest]
    #[case::read_write(AccessMode::ReadWrite)]
    #[case::read_only(AccessMode::ReadOnly)]
    #[tokio::test]
    async fn rejects_fsdir_before_reading_host_files(#[case] access_mode: AccessMode) {
        let (dir, dsn) = test_support::make_sqlite_db("");
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, "fsdir canary").unwrap();
        let adapter = SqliteAdapter::new();
        let sql = format!(
            "SELECT data FROM 'fsdir'('{}') WHERE name = '{}'",
            outside.display(),
            outside.display()
        );

        let result = adapter.execute_adhoc(&dsn, &sql, access_mode).await;

        assert!(matches!(
            result,
            Err(DbOperationError::UnsupportedOperation(details))
                if details == "SQLite fsdir access is not supported in safe mode"
        ));
    }
}
