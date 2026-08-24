use std::time::Instant;

use async_trait::async_trait;

use crate::adapters::csv_export::export_to_downloads;
use crate::app::ports::outbound::{AccessMode, DbOperationError, QueryExecutor};
use crate::domain::{
    CommandTag, QueryResult, QuerySource, TableKind, TableKindInfo, WriteExecutionResult,
    sqlite_sql::is_sqlite_explain_query_plan_sql,
};

use super::sqlite3::parser::{
    SqliteStatementPlan, aggregate_sqlite_command_tag, append_changes_query_for_plan,
    command_tag_result, is_sqlite_rerunnable_export_query, last_sqlite_result_set,
    parse_affected_rows, parse_count_result, quoted_to_query_result, reject_sqlite_fsdir,
    sqlite_adhoc_execution_query_for_plan, sqlite_empty_result_sentinel,
    sqlite_export_not_rerunnable_error, sqlite_probe_marker, sqlite_statement_plan,
    sqlite_statement_tags, statement_counts_as_select_tag, strip_sqlite_probes,
};
use super::{SqliteAdapter, sql};

#[async_trait]
impl QueryExecutor for SqliteAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, DbOperationError> {
        Self::validate_main_schema(schema)?;
        let path = Self::path_from_dsn(dsn)?;
        let (columns, order_columns, kind_info) = self.preview_metadata(path, table).await?;
        let rowid_order_alias =
            Self::preview_rowid_order_alias(&columns, &order_columns, &kind_info);
        let query = sql::build_preview_query(
            table,
            &columns,
            &order_columns,
            rowid_order_alias,
            limit,
            offset,
        );
        let result = self
            .execute_quoted_query(path, &query, QuerySource::Preview, true)
            .await?;
        Ok(result.with_columns_if_empty(columns))
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        let path = Self::path_from_dsn(dsn)?;
        let plan = sqlite_statement_plan(query)?;
        let marker = sqlite_probe_marker();
        let execution_query = sqlite_adhoc_execution_query_for_plan(&plan, &marker);

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self
            .execute_quote_for_query_plan(path, &execution_query, query, access_mode.is_read_only())
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let (stdout, changes) = strip_sqlite_probes(&stdout, &marker)?;
        let stdout = last_sqlite_result_set(&stdout, &marker)?.unwrap_or(stdout);
        let statements = plan.statements();
        let tag = aggregate_sqlite_command_tag(&sqlite_statement_tags(statements, &changes));

        if stdout.trim().is_empty() {
            if let Some(tag) = tag {
                return Ok(command_tag_result(query, tag, elapsed, QuerySource::Adhoc));
            }
            let mut result = QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                QuerySource::Adhoc,
            );
            if statements
                .iter()
                .any(|stmt| statement_counts_as_select_tag(stmt))
            {
                result = result.with_command_tag(CommandTag::Select(0));
            }
            return Ok(result);
        }

        let mut result = quoted_to_query_result(query, &stdout, QuerySource::Adhoc, elapsed)?;
        let empty_sentinel = sqlite_empty_result_sentinel(&marker);
        if result
            .columns
            .last()
            .is_some_and(|column| column == &empty_sentinel)
        {
            result = result.without_empty_result_sentinel();
        }
        if let Some(tag) = tag {
            result = result.with_command_tag(tag);
        } else if statements
            .iter()
            .any(|stmt| statement_counts_as_select_tag(stmt))
        {
            let row_count = result.row_count() as u64;
            result = result.with_command_tag(CommandTag::Select(row_count));
        }
        Ok(result)
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        let path = Self::path_from_dsn(dsn)?;
        let plan = sqlite_statement_plan(query)?;
        let affected_rows = self
            .execute_changes_query(path, &plan, access_mode.is_read_only())
            .await?;
        Ok(WriteExecutionResult {
            affected_rows,
            diagnostics: Vec::new(),
        })
    }

    async fn count_query_rows(&self, dsn: &str, query: &str) -> Result<usize, DbOperationError> {
        reject_sqlite_fsdir(query)?;
        let stdout = self
            .cli
            .execute_csv(Self::path_from_dsn(dsn)?, query, true)
            .await?;
        parse_count_result(&stdout)
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        if !is_sqlite_rerunnable_export_query(query)? {
            return Err(sqlite_export_not_rerunnable_error());
        }
        let database_path = Self::path_from_dsn(dsn)?.to_string();
        export_to_downloads(file_name, |path| async move {
            self.cli
                .export_csv(&database_path, query, &path, true)
                .await
        })
        .await
    }
}

impl SqliteAdapter {
    fn preview_rowid_order_alias(
        visible_columns: &[String],
        order_columns: &[String],
        kind_info: &TableKindInfo,
    ) -> Option<&'static str> {
        if !order_columns.is_empty() {
            return None;
        }
        if kind_info.kind != TableKind::Table || kind_info.without_rowid {
            return None;
        }
        ["rowid", "_rowid_", "oid"].into_iter().find(|alias| {
            !visible_columns
                .iter()
                .any(|column| column.eq_ignore_ascii_case(alias))
        })
    }

    async fn execute_quoted_query(
        &self,
        path: &str,
        query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self.cli.execute_quote(path, query, read_only).await?;
        let elapsed = start.elapsed().as_millis() as u64;
        quoted_to_query_result(query, &stdout, source, elapsed)
    }

    async fn execute_quote_for_query_plan(
        &self,
        path: &str,
        execution_sql: &str,
        source_sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        // Detect against source_sql because execution_sql may include probe statements.
        if is_sqlite_explain_query_plan_sql(source_sql) {
            self.cli
                .execute_quote_with_explain_off(path, execution_sql, read_only)
                .await
        } else {
            self.cli.execute_quote(path, execution_sql, read_only).await
        }
    }

    async fn execute_changes_query(
        &self,
        path: &str,
        plan: &SqliteStatementPlan<'_>,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        let stdout = self
            .cli
            .execute_csv(path, &append_changes_query_for_plan(plan), read_only)
            .await?;
        parse_affected_rows(&stdout)
    }
}

#[cfg(test)]
mod tests {
    use crate::app::ports::outbound::{AccessMode, SqlDialect};
    use crate::domain::{
        CommandTag, DatabaseType, QueryResult, QuerySource, QueryValue,
        sqlite_explain_query_plan_text_from_result,
    };

    use super::*;

    fn display_row(result: &QueryResult, row: usize) -> Vec<String> {
        result
            .display_row_at(row)
            .expect("test result should contain the requested row")
    }

    mod preview {
        use crate::adapters::test_support;

        use super::*;

        #[tokio::test]
        async fn metadata_and_rows_use_at_most_two_sqlite_processes() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
                CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT UNIQUE, org_id INTEGER);
                CREATE INDEX idx_users_org_id ON users(org_id);
                INSERT INTO users(id, email, org_id) VALUES (1, 'a@example.com', 7);
                ",
            );
            let (adapter, process_counter) = SqliteAdapter::with_process_counter(&dsn);

            adapter
                .execute_preview(&dsn, "main", "users", 10, 0)
                .await
                .unwrap();

            assert_eq!(process_counter.count(), 2);
        }

        #[tokio::test]
        async fn returns_columns_rows_and_respects_pagination() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (2, 'b'), (1, 'a'), (3, 'c');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 1, 1)
                .await
                .unwrap();

            assert_eq!(result.source, QuerySource::Preview);
            assert_eq!(result.columns, vec!["id", "name"]);
            assert_eq!(
                display_row(&result, 0),
                vec!["2".to_string(), "b".to_string()]
            );
        }

        #[tokio::test]
        async fn primary_keyless_preview_exposes_only_user_columns() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE logs(rowid TEXT, message TEXT);
            INSERT INTO logs(rowid, message) VALUES ('user-visible', 'first');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["rowid", "message"]);
            assert_eq!(
                display_row(&result, 0),
                vec!["user-visible".to_string(), "first".to_string()]
            );
        }

        #[tokio::test]
        async fn rejects_non_main_schema() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter.execute_preview(&dsn, "other", "users", 10, 0).await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        }

        #[tokio::test]
        async fn preserves_nul_text_primary_key_for_preview_and_delete() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(id TEXT PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES ('a' || char(0) || 'bc', 'target'), ('only', 'other');
            ",
            );
            let adapter = SqliteAdapter::new();

            let preview = adapter
                .execute_preview(&dsn, "main", "users", 10, 0)
                .await
                .unwrap();

            assert_eq!(
                preview.value_at(0, 0),
                Some(&QueryValue::Text("a\0bc".to_string()))
            );
            assert_eq!(preview.display_value_at(0, 0).as_deref(), Some("a\\0bc"));

            let delete_sql = adapter.build_bulk_delete_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                &[vec![(
                    "id".to_string(),
                    QueryValue::Text("a\0bc".to_string()),
                )]],
            );
            let write = adapter
                .execute_write(&dsn, &delete_sql, AccessMode::ReadWrite)
                .await
                .unwrap();
            assert_eq!(write.affected_rows, 1);

            let remaining = adapter
                .execute_preview(&dsn, "main", "users", 10, 0)
                .await
                .unwrap();
            assert_eq!(remaining.row_count(), 1);
            assert_eq!(
                remaining.value_at(0, 0),
                Some(&QueryValue::Text("only".to_string()))
            );
        }

        #[tokio::test]
        async fn excludes_hidden_columns_from_preview_select_list() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            INSERT INTO notes_fts(body) VALUES ('hello');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "notes_fts", 10, 0)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["body"]);
            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::Text("hello".to_string()))
            );
        }

        #[tokio::test]
        async fn empty_rowid_table_preview_keeps_all_visible_columns() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(name TEXT, email TEXT);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["name", "email"]);
            assert_eq!(result.data_row_count(), 0);
            assert_eq!(result.row_count(), 0);
        }

        #[tokio::test]
        async fn preserves_distinct_c0_text_values_in_preview() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, value TEXT);
            INSERT INTO users(value) VALUES (char(1) || char(1));
            INSERT INTO users(value) VALUES (char(1) || char(92) || 'u0001');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0)
                .await
                .unwrap();

            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::Text("\x01\x01".to_string()))
            );
            assert_eq!(
                result.value_at(1, 1),
                Some(&QueryValue::Text("\x01\\u0001".to_string()))
            );
        }

        #[tokio::test]
        async fn preserves_sentinel_like_text_without_nul_in_preview() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, token TEXT);
            INSERT INTO users(token) VALUES (char(1) || 'SABIQL_HEX:4142');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0)
                .await
                .unwrap();

            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::Text(format!(
                    "{}4142",
                    sql::sqlite_nul_text_sentinel()
                )))
            );
        }

        #[tokio::test]
        async fn keeps_generated_columns_in_preview_select_list() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                name TEXT,
                name_upper TEXT GENERATED ALWAYS AS (upper(name)) STORED
            );
            INSERT INTO users(name) VALUES ('alice');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["id", "name", "name_upper"]);
            assert_eq!(
                result.value_at(0, 2),
                Some(&QueryValue::Text("ALICE".to_string()))
            );
        }
    }

    mod adhoc_execution {
        use crate::adapters::test_support;

        use super::*;

        mod query_results {
            use super::*;

            #[tokio::test]
            async fn select_returns_query_result() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(&dsn, "SELECT 1 AS value", AccessMode::ReadOnly)
                    .await
                    .unwrap();

                assert_eq!(result.columns, vec!["value"]);
                assert_eq!(display_row(&result, 0), vec!["1".to_string()]);
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

                assert_eq!(display_row(&result, 0), vec!["1".to_string()]);
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

                let plan_text = sqlite_explain_query_plan_text_from_result(&result);

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

                let plan_text = sqlite_explain_query_plan_text_from_result(&result);
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

                let plan_text = sqlite_explain_query_plan_text_from_result(&result);

                assert!(
                    plan_text.to_ascii_lowercase().contains("users"),
                    "expected users table in plan, got: {plan_text}"
                );
                assert_eq!(display_row(&rows, 0), vec!["2".to_string()]);
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
                let (_dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT);",
                );
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
                    display_row(&result, 0),
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
                    display_row(&result, 0),
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
                    display_row(&result, 0),
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

                assert_eq!(display_row(&result, 0), vec!["1".to_string()]);
            }

            #[tokio::test]
            async fn read_only_session_enables_query_only_before_user_sql() {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(&dsn, "PRAGMA query_only", AccessMode::ReadOnly)
                    .await
                    .unwrap();

                assert_eq!(display_row(&result, 0), vec!["1".to_string()]);
            }

            #[tokio::test]
            async fn applies_busy_timeout_before_user_sql() {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(&dsn, "PRAGMA busy_timeout", AccessMode::ReadOnly)
                    .await
                    .unwrap();

                assert_eq!(display_row(&result, 0), vec!["5000".to_string()]);
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
                    "load_extension" => {
                        "SELECT load_extension('/tmp/sabiql-extension')".to_string()
                    }
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

        mod command_tags {
            use super::*;

            #[tokio::test]
            async fn values_result_does_not_get_select_command_tag() {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(&dsn, "VALUES (1)", AccessMode::ReadOnly)
                    .await
                    .unwrap();

                assert_eq!(display_row(&result, 0), vec!["1".to_string()]);
                assert_eq!(result.command_tag, None);
            }

            #[tokio::test]
            async fn dml_returns_affected_rows_command_tag() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "UPDATE users SET name = 'x' WHERE id = 1",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
            }

            #[tokio::test]
            async fn replace_into_returns_insert_refresh_tag() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "REPLACE INTO users(id, name) VALUES (1, 'z')",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            }

            #[tokio::test]
            async fn dml_with_following_select_uses_trailing_changes_result() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "UPDATE users SET name = 'x' WHERE id = 1; SELECT 42",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
            }

            #[tokio::test]
            async fn dml_with_following_select_preserves_result_set_and_refresh_tag() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "UPDATE users SET name = 'x' WHERE id = 1; SELECT name FROM users",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.columns, vec!["name"]);
                assert_eq!(display_row(&result, 0), vec!["x".to_string()]);
                assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
            }

            #[tokio::test]
            async fn multi_dml_uses_last_effective_refresh_tag() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "INSERT INTO users(id, name) VALUES (2, 'b'), (3, 'c');
                     UPDATE users SET name = 'z' WHERE id IN (1, 2);
                     DELETE FROM users WHERE id = 3",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
            }

            #[tokio::test]
            async fn ddl_wins_over_later_dml_for_refresh_tag() {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "CREATE TABLE users(id INTEGER PRIMARY KEY);
                     INSERT INTO users(id) VALUES (1)",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(
                    result.command_tag,
                    Some(CommandTag::Create("TABLE".to_string()))
                );
                assert_eq!(result.row_count(), 0);
            }
        }

        mod transaction_execution {
            use super::*;

            mod automatic_transactions {
                use super::*;

                #[tokio::test]
                async fn ddl_and_dml_still_roll_back_as_one_auto_transaction() {
                    let (_dir, dsn) = test_support::make_sqlite_db("");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "CREATE TABLE users(id INTEGER PRIMARY KEY);\
                     INSERT INTO users(id) VALUES (1);\
                     INSERT INTO missing(id) VALUES (2)",
                            AccessMode::ReadWrite,
                        )
                        .await;

                    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
                    let tables = adapter
                    .execute_adhoc(
                        &dsn,
                        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'users'",
                        AccessMode::ReadOnly,
                    )
                    .await
                    .unwrap();
                    assert_eq!(tables.data_row_count(), 0);
                }

                #[tokio::test]
                async fn persistent_pragma_writes_roll_back_when_a_later_statement_fails() {
                    for (write, read) in [
                        ("PRAGMA user_version = 42", "PRAGMA user_version"),
                        (
                            "PRAGMA \"main\".\"user_version\" = 42",
                            "PRAGMA user_version",
                        ),
                        ("PRAGMA [main].[application_id](7)", "PRAGMA application_id"),
                    ] {
                        let (_dir, dsn) = test_support::make_sqlite_db("");
                        let adapter = SqliteAdapter::new();

                        let result = adapter
                            .execute_adhoc(
                                &dsn,
                                &format!("{write}; SELECT * FROM missing_table"),
                                AccessMode::ReadWrite,
                            )
                            .await;

                        assert!(
                            matches!(result, Err(DbOperationError::ObjectMissing(_))),
                            "{write}"
                        );
                        let value = adapter
                            .execute_adhoc(&dsn, read, AccessMode::ReadOnly)
                            .await
                            .unwrap();
                        assert_eq!(display_row(&value, 0), vec!["0".to_string()], "{write}");
                    }
                }

                #[tokio::test]
                async fn vacuum_is_rejected_in_safe_mode() {
                    let (_dir, dsn) = test_support::make_sqlite_db("");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(&dsn, "VACUUM", AccessMode::ReadWrite)
                        .await;

                    // VACUUM internally attaches a temporary database, so safe mode reports ATTACH.
                    assert!(matches!(
                        result,
                        Err(DbOperationError::QueryFailed(details))
                            if details.contains("cannot run ATTACH in safe mode")
                    ));
                }

                #[tokio::test]
                async fn journal_mode_change_in_mixed_sql_runs_outside_auto_transaction() {
                    let (_dir, dsn) = test_support::make_sqlite_db("");
                    let adapter = SqliteAdapter::new();

                    adapter
                        .execute_adhoc(
                            &dsn,
                            "PRAGMA journal_mode = WAL;\
                     CREATE TABLE users(id INTEGER PRIMARY KEY);\
                     INSERT INTO users(id) VALUES (1)",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(display_row(&rows, 0), vec!["1".to_string()]);
                }

                #[tokio::test]
                async fn foreign_keys_change_in_mixed_sql_is_not_a_transaction_noop() {
                    let (_dir, dsn) = test_support::make_sqlite_db("");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "PRAGMA foreign_keys = ON;
                     CREATE TABLE parent(id INTEGER PRIMARY KEY);
                     PRAGMA foreign_keys",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();

                    assert_eq!(display_row(&result, 0), vec!["1".to_string()]);
                }
            }

            mod savepoint_rollbacks {
                use super::*;

                #[tokio::test]
                async fn rolled_back_dml_returns_rollback_tag() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "BEGIN; INSERT INTO users(id) VALUES (1); ROLLBACK",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(result.command_tag, Some(CommandTag::Rollback));
                    assert_eq!(rows.data_row_count(), 0);
                }

                #[tokio::test]
                async fn full_rollback_inside_savepoint_discards_outer_dml() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(result.command_tag, Some(CommandTag::Rollback));
                    assert_eq!(rows.data_row_count(), 0);
                }

                #[tokio::test]
                async fn savepoint_rollback_discards_inner_dml_only() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     COMMIT",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
                    assert_eq!(display_row(&rows, 0), vec!["1".to_string()]);
                }

                #[tokio::test]
                async fn rollback_to_keeps_savepoint_for_later_rollback() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     INSERT INTO users(id) VALUES (3);
                     ROLLBACK TO sp;
                     COMMIT",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
                    assert_eq!(display_row(&rows, 0), vec!["1".to_string()]);
                }

                #[tokio::test]
                async fn rollback_to_named_outer_savepoint_discards_nested_frames() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT outer_sp;
                     INSERT INTO users(id) VALUES (2);
                     SAVEPOINT inner_sp;
                     INSERT INTO users(id) VALUES (3);
                     ROLLBACK TO outer_sp;
                     COMMIT",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
                    assert_eq!(display_row(&rows, 0), vec!["1".to_string()]);
                }

                #[tokio::test]
                async fn top_level_savepoint_rollback_to_discards_inner_dml_only() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (1);
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK TO sp;
                     INSERT INTO users(id) VALUES (3);
                     RELEASE sp",
                            AccessMode::ReadWrite,
                        )
                        .await
                        .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
                    assert_eq!(display_row(&rows, 0), vec!["3".to_string()]);
                }

                #[tokio::test]
                async fn top_level_savepoint_multi_write_rolls_back_when_later_statement_fails() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SAVEPOINT sp; INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)", AccessMode::ReadWrite)
                .await;

                    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();
                    assert_eq!(rows.data_row_count(), 0);
                }

                #[tokio::test]
                async fn top_level_savepoint_without_release_does_not_persist_on_success() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SAVEPOINT sp; INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)", AccessMode::ReadWrite)
                .await
                .unwrap();
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();

                    assert_eq!(result.command_tag, Some(CommandTag::Rollback));
                    assert_eq!(rows.data_row_count(), 0);
                }
            }

            mod multi_statement_atomicity {
                use super::*;

                #[tokio::test]
                async fn multi_statement_dml_rolls_back_when_later_statement_fails() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)",
                            AccessMode::ReadWrite,
                        )
                        .await;

                    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();
                    assert_eq!(rows.data_row_count(), 0);
                }

                #[tokio::test]
                async fn with_dml_rolls_back_when_later_statement_fails() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                        .execute_adhoc(
                            &dsn,
                            "WITH payload(id) AS (VALUES (1))
                     INSERT INTO users(id) SELECT id FROM payload;
                     INSERT INTO missing(id) VALUES (2)",
                            AccessMode::ReadWrite,
                        )
                        .await;

                    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();
                    assert_eq!(rows.data_row_count(), 0);
                }

                #[tokio::test]
                async fn returning_dml_rolls_back_when_later_statement_fails() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();
                    let query = "INSERT INTO users(id) VALUES (1) RETURNING id; INSERT INTO missing(id) VALUES (2)";

                    let result = adapter
                        .execute_adhoc(&dsn, query, AccessMode::ReadWrite)
                        .await;

                    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();
                    assert_eq!(rows.data_row_count(), 0);
                }

                #[tokio::test]
                async fn select_then_dml_rolls_back_when_later_statement_fails() {
                    let (_dir, dsn) =
                        test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                    let adapter = SqliteAdapter::new();

                    let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT 1 AS marker; INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)", AccessMode::ReadWrite)
                .await;

                    assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
                    let rows = adapter
                        .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                        .await
                        .unwrap();
                    assert_eq!(rows.data_row_count(), 0);
                }
            }
        }

        mod returning_results {
            use super::*;

            #[tokio::test]
            async fn dml_returning_preserves_returned_rows() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "INSERT INTO users(name) VALUES ('a') RETURNING id, name",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.columns, vec!["id", "name"]);
                assert_eq!(
                    display_row(&result, 0),
                    vec!["1".to_string(), "a".to_string()]
                );
                assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            }

            #[tokio::test]
            async fn dml_returning_preserves_empty_suffix_column() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "INSERT INTO users(name) VALUES ('a') RETURNING id AS value_empty",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.columns, vec!["value_empty"]);
                assert_eq!(display_row(&result, 0), vec!["1".to_string()]);
                assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            }

            #[tokio::test]
            async fn update_returning_preserves_returned_rows() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "UPDATE users SET name = 'x' RETURNING id, name",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.data_row_count(), 2);
                assert_eq!(result.command_tag, Some(CommandTag::Update(2)));
            }

            #[tokio::test]
            async fn delete_returning_preserves_returned_rows() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "DELETE FROM users WHERE id = 1 RETURNING id, name",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(
                    display_row(&result, 0),
                    vec!["1".to_string(), "a".to_string()]
                );
                assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
            }
        }

        mod dml_command_tags {
            use super::*;

            #[tokio::test]
            async fn dml_with_trailing_line_comment_returns_affected_rows() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "DELETE FROM users WHERE id = 1 -- cleanup selected row",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
            }

            #[tokio::test]
            async fn with_insert_reports_affected_rows_command_tag() {
                let (_dir, dsn) =
                    test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "WITH payload(id) AS (VALUES (1), (2))
                     INSERT INTO users(id) SELECT id FROM payload",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 2);
                assert_eq!(result.command_tag, Some(CommandTag::Insert(2)));
            }

            #[tokio::test]
            async fn dml_table_name_containing_returning_reports_affected_rows() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE returning_log(id INTEGER PRIMARY KEY, name TEXT);",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "INSERT INTO returning_log(name) VALUES ('a')",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            }

            #[tokio::test]
            async fn dml_backtick_quoted_identifier_containing_returning_reports_affected_rows() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE `my returning`(id INTEGER PRIMARY KEY, name TEXT);",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "INSERT INTO `my returning`(name) VALUES ('a')",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            }

            #[tokio::test]
            async fn dml_bracket_quoted_identifier_containing_returning_reports_affected_rows() {
                let (_dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE [my returning](id INTEGER PRIMARY KEY, name TEXT);",
                );
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "INSERT INTO [my returning](name) VALUES ('a')",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(result.row_count(), 1);
                assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            }
        }

        mod ddl_command_tags {
            use super::*;

            #[tokio::test]
            async fn ddl_returns_schema_refresh_command_tag() {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "CREATE TABLE users(id INTEGER PRIMARY KEY)",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();

                assert_eq!(
                    result.command_tag,
                    Some(CommandTag::Create("TABLE".to_string()))
                );
            }
        }
    }

    mod write_execution {
        use crate::adapters::test_support;

        use super::*;

        #[tokio::test]
        async fn foreign_key_restrict_rejects_parent_delete_with_child_row() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                org_id INTEGER REFERENCES orgs(id) ON DELETE RESTRICT
            );
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, org_id) VALUES (1, 1);
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", AccessMode::ReadWrite)
                .await;
            let children = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert!(matches!(
                result,
                Err(DbOperationError::ForeignKeyViolation(_))
            ));
            assert_eq!(display_row(&children, 0), vec!["1".to_string()]);
        }

        #[tokio::test]
        async fn unique_constraint_violation_is_classified() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT UNIQUE NOT NULL);",
            );
            let adapter = SqliteAdapter::new();

            adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO users(id, email) VALUES (1, 'a@example.com')",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();

            let result = adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO users(id, email) VALUES (2, 'a@example.com')",
                    AccessMode::ReadWrite,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::UniqueViolation(_))));
        }

        #[tokio::test]
        async fn syntax_error_stays_query_failed_with_details() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELEKT 1", AccessMode::ReadOnly)
                .await;

            assert!(matches!(result, Err(DbOperationError::QueryFailed(message))
                    if message.to_ascii_lowercase().contains("syntax error")));
        }

        #[tokio::test]
        async fn foreign_key_cascade_applies_to_parent_delete() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE orgs(id INTEGER PRIMARY KEY);
            CREATE TABLE users(
                id INTEGER PRIMARY KEY,
                org_id INTEGER REFERENCES orgs(id) ON DELETE CASCADE
            );
            INSERT INTO orgs(id) VALUES (1);
            INSERT INTO users(id, org_id) VALUES (1, 1);
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", AccessMode::ReadWrite)
                .await
                .unwrap();
            let children = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", AccessMode::ReadOnly)
                .await
                .unwrap();

            assert_eq!(result.affected_rows, 1);
            assert_eq!(children.data_row_count(), 0);
        }

        #[tokio::test]
        async fn returns_affected_rows() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(
                    &dsn,
                    "DELETE FROM users WHERE id IN (1, 2)",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();

            assert_eq!(result.affected_rows, 2);
        }

        #[tokio::test]
        async fn count_query_rows_parses_count_result() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            INSERT INTO users(id) VALUES (1), (2), (3);
            ",
            );
            let adapter = SqliteAdapter::new();

            let count = adapter
                .count_query_rows(&dsn, "SELECT COUNT(*) FROM users")
                .await
                .unwrap();

            assert_eq!(count, 3);
        }

        #[tokio::test]
        async fn count_query_rows_rejects_fsdir_before_execution() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .count_query_rows(
                    &dsn,
                    "SELECT COUNT(*) FROM (SELECT * FROM fsdir('/tmp')) AS files",
                )
                .await;

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(details))
                    if details == "SQLite fsdir access is not supported in safe mode"
            ));
        }

        #[tokio::test]
        async fn export_to_csv_writes_rows() {
            let (dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let path = dir.path().join("users.csv");
            let adapter = SqliteAdapter::new();

            adapter
                .cli
                .export_csv(
                    SqliteAdapter::path_from_dsn(&dsn).unwrap(),
                    "SELECT id, name FROM users ORDER BY id",
                    &path,
                    true,
                )
                .await
                .unwrap();
            let csv = std::fs::read_to_string(path).unwrap();

            assert_eq!(csv, "id,name\n1,a\n2,b\n");
        }

        #[tokio::test]
        async fn export_to_csv_runs_sql_larger_than_the_windows_command_line_limit() {
            let (dir, dsn) = test_support::make_sqlite_db("");
            let path = dir.path().join("long.csv");
            let adapter = SqliteAdapter::new();
            let sql = format!("SELECT 1 AS value /* {} */", "x".repeat(32_768));

            adapter
                .cli
                .export_csv(
                    SqliteAdapter::path_from_dsn(&dsn).unwrap(),
                    &sql,
                    &path,
                    true,
                )
                .await
                .unwrap();

            assert_eq!(std::fs::read_to_string(path).unwrap(), "value\n1\n");
        }

        #[tokio::test]
        async fn export_to_csv_preserves_records_with_embedded_newlines() {
            let (dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE logs(id INTEGER PRIMARY KEY, message TEXT);
            INSERT INTO logs(id, message) VALUES (1, 'hello
world'), (2, 'done');
            ",
            );
            let path = dir.path().join("logs.csv");
            let adapter = SqliteAdapter::new();

            adapter
                .cli
                .export_csv(
                    SqliteAdapter::path_from_dsn(&dsn).unwrap(),
                    "SELECT id, message FROM logs ORDER BY id",
                    &path,
                    true,
                )
                .await
                .unwrap();

            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                "id,message\n1,\"hello\nworld\"\n2,done\n"
            );
        }

        #[tokio::test]
        async fn export_to_csv_rejects_write_sql() {
            let (dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let path = dir.path().join("write_export.csv");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .export_to_csv(&dsn, "INSERT INTO users(id) VALUES (1)", "write_export")
                .await;

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(message))
                if message.contains("write or DDL")
            ));
            assert!(!path.exists());
        }

        #[tokio::test]
        async fn export_to_csv_rejects_fsdir_before_creating_output() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .export_to_csv(&dsn, "SELECT * FROM fsdir('/tmp')", "fsdir_export")
                .await;

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(details))
                    if details == "SQLite fsdir access is not supported in safe mode"
            ));
        }

        #[tokio::test]
        async fn export_to_csv_missing_table_returns_object_missing_and_removes_file() {
            let (dir, dsn) = test_support::make_sqlite_db("");
            let path = dir.path().join("missing_export.csv");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .export_to_csv(&dsn, "SELECT id FROM missing", "missing_export")
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            assert!(!path.exists());
        }

        #[tokio::test]
        async fn count_query_rows_missing_table_returns_object_missing() {
            let (_dir, dsn) = test_support::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .count_query_rows(&dsn, "SELECT COUNT(*) FROM missing")
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        }

        #[tokio::test]
        async fn read_only_write_fails() {
            let (_dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO users(id) VALUES (1)",
                    AccessMode::ReadOnly,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::PermissionDenied(_))));
        }

        #[tokio::test]
        async fn missing_database_is_rejected_without_creating_an_empty_file() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("missing.db");
            let dsn = format!("sqlite://{}", path.display());
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(
                    &dsn,
                    "CREATE TABLE users(id INTEGER)",
                    AccessMode::ReadWrite,
                )
                .await;

            assert!(matches!(
                result,
                Err(DbOperationError::ConnectionFailed(details))
                    if details.contains("SQLite database file not found")
            ));
            assert!(!path.exists());
        }
    }
}
