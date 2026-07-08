use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, QueryExecutor};
use crate::domain::{
    CommandTag, QueryResult, QuerySource, TableKind, WriteExecutionResult,
    available_sqlite_rowid_alias,
};

use super::super::{SqliteAdapter, sql};
use super::error::classify_query_error;
use super::parser::{
    aggregate_sqlite_command_tag, append_changes_query, command_tag_result,
    is_sqlite_rerunnable_export_query, last_sqlite_result_set, parse_affected_rows,
    parse_count_result, quoted_to_query_result, sqlite_adhoc_execution_query,
    sqlite_export_not_rerunnable_error, sqlite_probe_marker, sqlite_statement_tags,
    statement_counts_as_select_tag, strip_sqlite_probes, try_split_sqlite_statements,
};

pub(in crate::adapters::sqlite) const BUSY_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone)]
pub(in crate::adapters::sqlite) struct SqliteCli {
    timeout_secs: u64,
}

struct SqliteOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl SqliteCli {
    pub(in crate::adapters::sqlite) fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    pub(in crate::adapters::sqlite) async fn execute_json<T: DeserializeOwned>(
        &self,
        path: &str,
        sql: &str,
    ) -> Result<T, DbOperationError> {
        let output = self.run(path, &["-json"], sql, true).await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        let stdout = match output.stdout.trim() {
            "" => "[]",
            stdout => stdout,
        };
        serde_json::from_str(stdout).map_err(DbOperationError::from)
    }

    pub(in crate::adapters::sqlite) async fn execute_csv(
        &self,
        path: &str,
        sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        let output = self
            .run(
                path,
                &["-batch", "-bail", "-csv", "-header"],
                sql,
                read_only,
            )
            .await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        Ok(output.stdout)
    }

    pub(in crate::adapters::sqlite) async fn execute_quote(
        &self,
        path: &str,
        sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        let output = self
            .run(
                path,
                &["-batch", "-bail", "-quote", "-header"],
                sql,
                read_only,
            )
            .await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        Ok(output.stdout)
    }

    pub(in crate::adapters::sqlite) async fn export_csv(
        &self,
        path: &str,
        sql: &str,
        output_path: &std::path::Path,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        let mut cmd = Command::new("sqlite3");
        Self::apply_session_options(&mut cmd, read_only);
        cmd.arg("-batch").arg("-bail").arg("-csv").arg("-header");
        cmd.arg("--").arg(path).arg(sql);

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| DbOperationError::CommandNotFound(error.to_string()))?;

        let stdout = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let file = tokio::fs::File::create(output_path)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let mut writer = tokio::io::BufWriter::new(file);

        let result = timeout(Duration::from_secs(self.timeout_secs * 10), async {
            if let Some(mut stdout) = stdout {
                let mut buf = [0u8; 8192];
                loop {
                    let n = stdout.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n]).await?;
                }
                writer.flush().await?;
            }

            let stderr = {
                let mut buf = Vec::new();
                if let Some(ref mut stderr) = stderr_handle {
                    stderr.read_to_end(&mut buf).await?;
                }
                String::from_utf8_lossy(&buf).into_owned()
            };
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stderr))
        })
        .await;

        let (status, stderr) = match result {
            Ok(inner) => match inner {
                Ok(values) => values,
                Err(error) => {
                    let _ = tokio::fs::remove_file(output_path).await;
                    return Err(DbOperationError::QueryFailed(error.to_string()));
                }
            },
            Err(error) => {
                let _ = tokio::fs::remove_file(output_path).await;
                return Err(DbOperationError::Timeout(error.to_string()));
            }
        };

        if !status.success() {
            let _ = tokio::fs::remove_file(output_path).await;
            return Err(classify_query_error(&stderr));
        }

        match count_csv_records_async(output_path).await {
            Ok(row_count) => Ok(row_count),
            Err(error) => {
                let _ = tokio::fs::remove_file(output_path).await;
                Err(error)
            }
        }
    }

    async fn run(
        &self,
        path: &str,
        args: &[&str],
        sql: &str,
        read_only: bool,
    ) -> Result<SqliteOutput, DbOperationError> {
        let mut cmd = Command::new("sqlite3");
        Self::apply_session_options(&mut cmd, read_only);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.arg("--").arg(path).arg(sql);
        Self::collect_output(&mut cmd, self.timeout_secs).await
    }

    fn apply_session_options(cmd: &mut Command, read_only: bool) {
        if read_only {
            cmd.arg("-readonly");
        }
        cmd.arg("-cmd")
            .arg(format!(".timeout {BUSY_TIMEOUT_MS}"))
            .arg("-cmd")
            .arg("PRAGMA foreign_keys=ON");
        if read_only {
            cmd.arg("-cmd").arg("PRAGMA query_only=ON");
        }
    }

    async fn collect_output(
        cmd: &mut Command,
        timeout_secs: u64,
    ) -> Result<SqliteOutput, DbOperationError> {
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| DbOperationError::CommandNotFound(error.to_string()))?;

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let result = timeout(Duration::from_secs(timeout_secs), async {
            let (stdout_result, stderr_result) = tokio::join!(
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut stdout) = stdout_handle {
                        stdout.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                },
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut stderr) = stderr_handle {
                        stderr.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                }
            );

            let stdout = stdout_result?;
            let stderr = stderr_result?;
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stdout, stderr))
        })
        .await
        .map_err(|error| DbOperationError::Timeout(error.to_string()))?
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;

        let (status, stdout, stderr) = result;
        Ok(SqliteOutput {
            status,
            stdout,
            stderr,
        })
    }
}

fn count_csv_records(path: &std::path::Path) -> Result<usize, csv::Error> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;
    reader
        .records()
        .try_fold(0usize, |count, record| record.map(|_| count + 1))
}

async fn count_csv_records_async(path: &std::path::Path) -> Result<usize, DbOperationError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || count_csv_records(&path))
        .await
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
}

impl SqliteAdapter {
    async fn preview_rowid_alias(
        &self,
        path: &str,
        table: &str,
        visible_columns: &[String],
        order_columns: &[String],
    ) -> Option<&'static str> {
        if !order_columns.is_empty() {
            return None;
        }
        let kind_info = self.table_kind_info(path, table).await.ok().flatten()?;
        if kind_info.kind != TableKind::Table || kind_info.without_rowid {
            return None;
        }
        available_sqlite_rowid_alias(visible_columns.iter().map(String::as_str))
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

    async fn execute_changes_query(
        &self,
        path: &str,
        query: &str,
        read_only: bool,
    ) -> Result<(usize, u64), DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self
            .cli
            .execute_csv(path, &append_changes_query(query)?, read_only)
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        Ok((parse_affected_rows(&stdout)?, elapsed))
    }
}

#[async_trait]
impl QueryExecutor for SqliteAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        Self::validate_main_schema(schema)?;
        let path = Self::path_from_dsn(dsn)?;
        let order_columns = self.preview_order_columns(path, table).await;
        let columns = self
            .preview_visible_column_names(path, table)
            .await
            .unwrap_or_default();
        let rowid_alias = self
            .preview_rowid_alias(path, table, &columns, &order_columns)
            .await;
        let query =
            sql::build_preview_query(table, &columns, &order_columns, rowid_alias, limit, offset);
        let result = self
            .execute_quoted_query(path, &query, QuerySource::Preview, read_only)
            .await?;
        Ok(match rowid_alias {
            Some(alias) => result.with_first_column_hidden(alias.to_string()),
            None => result,
        })
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        let path = Self::path_from_dsn(dsn)?;
        let marker = sqlite_probe_marker();
        let statements = try_split_sqlite_statements(query)?;
        let execution_query = sqlite_adhoc_execution_query(query, &marker)?;

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self
            .cli
            .execute_quote(path, &execution_query, read_only)
            .await?;
        let elapsed = start.elapsed().as_millis() as u64;
        let (stdout, changes) = strip_sqlite_probes(&stdout, &marker)?;
        let stdout = last_sqlite_result_set(&stdout, &marker)?.unwrap_or(stdout);
        let tag = aggregate_sqlite_command_tag(&sqlite_statement_tags(&statements, &changes));

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
        read_only: bool,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        let (affected_rows, execution_time_ms) = self
            .execute_changes_query(Self::path_from_dsn(dsn)?, query, read_only)
            .await?;
        Ok(WriteExecutionResult {
            affected_rows,
            execution_time_ms,
        })
    }

    async fn count_query_rows(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        let stdout = self
            .cli
            .execute_csv(Self::path_from_dsn(dsn)?, query, read_only)
            .await?;
        parse_count_result(&stdout)
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        path: &std::path::Path,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        if !is_sqlite_rerunnable_export_query(query)? {
            return Err(sqlite_export_not_rerunnable_error());
        }
        self.cli
            .export_csv(Self::path_from_dsn(dsn)?, query, path, read_only)
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::app::ports::outbound::{QueryExecutor, SqlDialect};
    use crate::domain::{CommandTag, DatabaseType, QuerySource, QueryValue};

    use super::*;

    mod preview {
        use super::*;

        #[tokio::test]
        async fn returns_columns_rows_and_respects_pagination() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (2, 'b'), (1, 'a'), (3, 'c');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 1, 1, true)
                .await
                .unwrap();

            assert_eq!(result.source, QuerySource::Preview);
            assert_eq!(result.columns, vec!["id", "name"]);
            assert_eq!(result.rows(), vec![vec!["2".to_string(), "b".to_string()]]);
        }

        #[tokio::test]
        async fn rowid_table_preview_hides_rowid_but_keeps_internal_identity() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE logs(message TEXT);
            INSERT INTO logs(message) VALUES ('first'), ('second');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["message"]);
            assert_eq!(result.rows()[0], vec!["first".to_string()]);
            assert_eq!(
                result.hidden_value_at(0, "rowid"),
                Some(&QueryValue::SqlLiteral("1".to_string()))
            );
        }

        #[tokio::test]
        async fn rowid_table_preview_uses_unshadowed_rowid_alias() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE logs(rowid TEXT, message TEXT);
            INSERT INTO logs(rowid, message) VALUES ('user-visible', 'first');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["rowid", "message"]);
            assert_eq!(
                result.rows()[0],
                vec!["user-visible".to_string(), "first".to_string()]
            );
            assert_eq!(
                result.hidden_value_at(0, "_rowid_"),
                Some(&QueryValue::SqlLiteral("1".to_string()))
            );
        }

        #[tokio::test]
        async fn rowid_update_predicate_updates_matching_current_row() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE logs(message TEXT);
            INSERT INTO logs(message) VALUES ('old');
            ",
            );
            let adapter = SqliteAdapter::new();
            let predicate = vec![
                ("rowid".to_string(), QueryValue::SqlLiteral("1".to_string())),
                ("message".to_string(), QueryValue::text("old")),
            ];

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "logs",
                "message",
                &QueryValue::text("new"),
                &predicate,
            );
            let write = adapter.execute_write(&dsn, &sql, false).await.unwrap();
            let preview = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(write.affected_rows, 1);
            assert_eq!(preview.rows(), vec![vec!["new".to_string()]]);
        }

        #[tokio::test]
        async fn rowid_update_predicate_rejects_reused_rowid_with_changed_values() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE logs(message TEXT, note TEXT);
            INSERT INTO logs(message, note) VALUES ('old', NULL);
            ",
            );
            let adapter = SqliteAdapter::new();
            let stale_pairs = vec![
                ("rowid".to_string(), QueryValue::SqlLiteral("1".to_string())),
                ("message".to_string(), QueryValue::text("old")),
                ("note".to_string(), QueryValue::Null),
            ];

            adapter
                .execute_write(&dsn, "DELETE FROM logs WHERE rowid = 1", false)
                .await
                .unwrap();
            adapter.execute_adhoc(&dsn, "VACUUM", false).await.unwrap();
            adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO logs(message, note) VALUES ('replacement', NULL)",
                    false,
                )
                .await
                .unwrap();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "logs",
                "message",
                &QueryValue::text("new"),
                &stale_pairs,
            );
            let write = adapter.execute_write(&dsn, &sql, false).await.unwrap();
            let remaining = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(write.affected_rows, 0);
            assert_eq!(
                remaining.rows(),
                vec![vec!["replacement".to_string(), "NULL".to_string()]]
            );
        }

        #[tokio::test]
        async fn rowid_delete_predicate_deletes_matching_current_row() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE logs(message TEXT);
            INSERT INTO logs(message) VALUES ('old');
            ",
            );
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![
                ("rowid".to_string(), QueryValue::SqlLiteral("1".to_string())),
                ("message".to_string(), QueryValue::text("old")),
            ]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "logs", &rows);
            let write = adapter.execute_write(&dsn, &sql, false).await.unwrap();
            let preview = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(write.affected_rows, 1);
            assert_eq!(preview.row_count(), 0);
        }

        #[tokio::test]
        async fn rowid_delete_predicate_rejects_reused_rowid_with_changed_values() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE logs(message TEXT);
            INSERT INTO logs(message) VALUES ('old');
            ",
            );
            let adapter = SqliteAdapter::new();
            let stale_rows = vec![vec![
                ("rowid".to_string(), QueryValue::SqlLiteral("1".to_string())),
                ("message".to_string(), QueryValue::text("old")),
            ]];

            adapter
                .execute_write(&dsn, "DELETE FROM logs WHERE rowid = 1", false)
                .await
                .unwrap();
            adapter.execute_adhoc(&dsn, "VACUUM", false).await.unwrap();
            adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO logs(message) VALUES ('replacement')",
                    false,
                )
                .await
                .unwrap();

            let sql =
                adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "logs", &stale_rows);
            let write = adapter.execute_write(&dsn, &sql, false).await.unwrap();
            let remaining = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(write.affected_rows, 0);
            assert_eq!(remaining.row_count(), 1);
            assert_eq!(remaining.rows(), vec![vec!["replacement".to_string()]]);
        }

        #[tokio::test]
        async fn rejects_non_main_schema() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "other", "users", 10, 0, true)
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        }

        #[tokio::test]
        async fn preserves_nul_text_primary_key_for_preview_and_delete() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id TEXT PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES ('a' || char(0) || 'bc', 'target'), ('only', 'other');
            ",
            );
            let adapter = SqliteAdapter::new();

            let preview = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(
                preview.value_at(0, 0),
                Some(&QueryValue::Text("a\0bc".to_string()))
            );
            assert_eq!(preview.rows()[0][0], "a\\0bc");

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
                .execute_write(&dsn, &delete_sql, false)
                .await
                .unwrap();
            assert_eq!(write.affected_rows, 1);

            let remaining = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
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
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE VIRTUAL TABLE notes_fts USING fts5(body);
            INSERT INTO notes_fts(body) VALUES ('hello');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "notes_fts", 10, 0, true)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["body"]);
            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::Text("hello".to_string()))
            );
        }

        #[tokio::test]
        async fn preserves_distinct_c0_text_values_in_preview() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, value TEXT);
            INSERT INTO users(value) VALUES (char(1) || char(1));
            INSERT INTO users(value) VALUES (char(1) || char(92) || 'u0001');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
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
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, token TEXT);
            INSERT INTO users(token) VALUES (char(1) || 'SABIQL_HEX:4142');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "users", 10, 0, true)
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
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                .execute_preview(&dsn, "main", "users", 10, 0, true)
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
        use super::*;

        #[tokio::test]
        async fn select_returns_query_result() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS value", true)
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["value"]);
            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn explain_query_plan_returns_readable_detail_lines() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
                 CREATE INDEX idx_users_name ON users(name);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "EXPLAIN QUERY PLAN SELECT * FROM users WHERE name = 'alice'",
                    true,
                )
                .await
                .unwrap();

            let plan_text = explain_plan_text_from_result(&result);

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
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);
                 CREATE TABLE orders(id INTEGER, user_id INTEGER);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "EXPLAIN QUERY PLAN SELECT * FROM users u JOIN orders o ON u.id = o.user_id",
                    true,
                )
                .await
                .unwrap();

            let plan_text = explain_plan_text_from_result(&result);
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

        fn explain_plan_text_from_result(result: &QueryResult) -> String {
            result
                .rows()
                .iter()
                .filter_map(|row| row.first())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
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
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(setup);
            let adapter = SqliteAdapter::new();

            adapter.execute_adhoc(&dsn, trigger, false).await.unwrap();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'agent_messages_fts_ai'",
                    true,
                )
                .await
                .unwrap();

            let stored = result.rows()[0][0].replace('\n', " ");
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
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(setup);
            let adapter = SqliteAdapter::new();

            adapter.execute_adhoc(&dsn, trigger, false).await.unwrap();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'sync_end'",
                    true,
                )
                .await
                .unwrap();

            let stored = result.rows()[0][0].replace('\n', " ");
            let expected = trigger.trim().replace('\n', " ");
            assert!(
                !stored.contains("__sabiql_sqlite_probe_"),
                "probe SQL must not appear in stored trigger definition: {stored}"
            );
            assert_eq!(stored, expected);
        }

        #[tokio::test]
        async fn unclosed_create_trigger_fails_before_execution() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let error = adapter
                .execute_adhoc(
                    &dsn,
                    "CREATE TRIGGER t AFTER INSERT ON users BEGIN INSERT INTO logs(id) VALUES (1);",
                    false,
                )
                .await
                .unwrap_err();

            assert!(matches!(error, DbOperationError::QueryFailed(_)));
        }

        #[tokio::test]
        async fn select_preserves_quoted_newline_in_multicolumn_result() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT 'line 1' || char(10) || 'line 2' AS body, 'ok' AS marker",
                    true,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn multi_select_preserves_quoted_newline_in_last_result() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
            .execute_adhoc(
                &dsn,
                "SELECT 1 AS ignored; SELECT 'line 1' || char(10) || 'line 2' AS body, 'ok' AS marker",
                true,
            )
            .await
            .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn multi_select_does_not_treat_data_row_as_next_header() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT 1 AS a, 2 AS b UNION ALL SELECT 3, 4; SELECT 5 AS c, 6 AS d",
                    true,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["c", "d"]);
            assert_eq!(result.rows(), vec![vec!["5".to_string(), "6".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
        }

        #[tokio::test]
        async fn multi_select_empty_trailing_result_returns_empty_result() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS a; SELECT 2 AS b WHERE false", true)
                .await
                .unwrap();

            assert!(result.columns.is_empty());
            assert!(result.rows().is_empty());
            assert_eq!(result.command_tag, Some(CommandTag::Select(0)));
        }

        #[tokio::test]
        async fn pragma_result_does_not_get_select_command_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA table_info(users)", true)
                .await
                .unwrap();

            assert_eq!(
                result.columns,
                vec!["cid", "name", "type", "notnull", "dflt_value", "pk"]
            );
            assert_eq!(result.command_tag, None);
        }

        #[tokio::test]
        async fn enables_foreign_keys_before_user_sql() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA foreign_keys", false)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn read_only_session_enables_query_only_before_user_sql() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA query_only", true)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn applies_busy_timeout_before_user_sql() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "PRAGMA busy_timeout", true)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec![BUSY_TIMEOUT_MS.to_string()]]);
        }

        #[tokio::test]
        async fn values_result_does_not_get_select_command_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "VALUES (1)", true)
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
            assert_eq!(result.command_tag, None);
        }

        #[tokio::test]
        async fn dml_returns_affected_rows_command_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "UPDATE users SET name = 'x' WHERE id = 1", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
        }

        #[tokio::test]
        async fn replace_into_returns_insert_refresh_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "REPLACE INTO users(id, name) VALUES (1, 'z')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn dml_with_following_select_uses_trailing_changes_result() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
        }

        #[tokio::test]
        async fn dml_with_following_select_preserves_result_set_and_refresh_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["name"]);
            assert_eq!(result.rows(), vec![vec!["x".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Update(1)));
        }

        #[tokio::test]
        async fn multi_dml_uses_last_effective_refresh_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
        }

        #[tokio::test]
        async fn ddl_wins_over_later_dml_for_refresh_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "CREATE TABLE users(id INTEGER PRIMARY KEY);
                     INSERT INTO users(id) VALUES (1)",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(
                result.command_tag,
                Some(CommandTag::Create("TABLE".to_string()))
            );
            assert_eq!(result.row_count(), 0);
        }

        #[tokio::test]
        async fn rolled_back_dml_returns_rollback_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN; INSERT INTO users(id) VALUES (1); ROLLBACK",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn full_rollback_inside_savepoint_discards_outer_dml() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "BEGIN;
                     INSERT INTO users(id) VALUES (1);
                     SAVEPOINT sp;
                     INSERT INTO users(id) VALUES (2);
                     ROLLBACK",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn savepoint_rollback_discards_inner_dml_only() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
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
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn rollback_to_keeps_savepoint_for_later_rollback() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
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
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn rollback_to_named_outer_savepoint_discards_nested_frames() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
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
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn top_level_savepoint_rollback_to_discards_inner_dml_only() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
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
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
            assert_eq!(rows.rows(), vec![vec!["3".to_string()]]);
        }

        #[tokio::test]
        async fn top_level_savepoint_multi_write_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SAVEPOINT sp; INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)",
                    false,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn top_level_savepoint_without_release_does_not_persist_on_success() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SAVEPOINT sp; INSERT INTO users(id) VALUES (1); INSERT INTO users(id) VALUES (2)",
                    false,
                )
                .await
                .unwrap();
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.command_tag, Some(CommandTag::Rollback));
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn with_insert_reports_affected_rows_command_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "WITH payload(id) AS (VALUES (1), (2))
                     INSERT INTO users(id) SELECT id FROM payload",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 2);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(2)));
        }

        #[tokio::test]
        async fn multi_statement_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)",
                    false,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn with_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "WITH payload(id) AS (VALUES (1))
                     INSERT INTO users(id) SELECT id FROM payload;
                     INSERT INTO missing(id) VALUES (2)",
                    false,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn returning_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();
            let query =
                "INSERT INTO users(id) VALUES (1) RETURNING id; INSERT INTO missing(id) VALUES (2)";

            let result = adapter.execute_adhoc(&dsn, query, false).await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn select_then_dml_rolls_back_when_later_statement_fails() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "SELECT 1 AS marker; INSERT INTO users(id) VALUES (1); INSERT INTO missing(id) VALUES (2)",
                    false,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            let rows = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();
            assert!(rows.rows().is_empty());
        }

        #[tokio::test]
        async fn dml_with_trailing_line_comment_returns_affected_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
        }

        #[tokio::test]
        async fn dml_returning_preserves_returned_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(
                    &dsn,
                    "INSERT INTO users(name) VALUES ('a') RETURNING id, name",
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.columns, vec!["id", "name"]);
            assert_eq!(result.rows(), vec![vec!["1".to_string(), "a".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn update_returning_preserves_returned_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.rows().len(), 2);
            assert_eq!(result.command_tag, Some(CommandTag::Update(2)));
        }

        #[tokio::test]
        async fn delete_returning_preserves_returned_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                    false,
                )
                .await
                .unwrap();

            assert_eq!(result.rows(), vec![vec!["1".to_string(), "a".to_string()]]);
            assert_eq!(result.command_tag, Some(CommandTag::Delete(1)));
        }

        #[tokio::test]
        async fn dml_table_name_containing_returning_reports_affected_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE returning_log(id INTEGER PRIMARY KEY, name TEXT);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "INSERT INTO returning_log(name) VALUES ('a')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn dml_backtick_quoted_identifier_containing_returning_reports_affected_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE `my returning`(id INTEGER PRIMARY KEY, name TEXT);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "INSERT INTO `my returning`(name) VALUES ('a')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn dml_bracket_quoted_identifier_containing_returning_reports_affected_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE [my returning](id INTEGER PRIMARY KEY, name TEXT);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "INSERT INTO [my returning](name) VALUES ('a')", false)
                .await
                .unwrap();

            assert_eq!(result.row_count(), 1);
            assert_eq!(result.command_tag, Some(CommandTag::Insert(1)));
        }

        #[tokio::test]
        async fn ddl_returns_schema_refresh_command_tag() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "CREATE TABLE users(id INTEGER PRIMARY KEY)", false)
                .await
                .unwrap();

            assert_eq!(
                result.command_tag,
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }
    }

    mod write_execution {
        use super::*;

        #[tokio::test]
        async fn foreign_key_restrict_rejects_parent_delete_with_child_row() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", false)
                .await;
            let children = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert!(matches!(
                result,
                Err(DbOperationError::ForeignKeyViolation(_))
            ));
            assert_eq!(children.rows(), vec![vec!["1".to_string()]]);
        }

        #[tokio::test]
        async fn unique_constraint_violation_is_classified() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT UNIQUE NOT NULL);",
            );
            let adapter = SqliteAdapter::new();

            adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO users(id, email) VALUES (1, 'a@example.com')",
                    false,
                )
                .await
                .unwrap();

            let result = adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO users(id, email) VALUES (2, 'a@example.com')",
                    false,
                )
                .await;

            assert!(matches!(result, Err(DbOperationError::UniqueViolation(_))));
        }

        #[tokio::test]
        async fn syntax_error_stays_query_failed_with_details() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter.execute_adhoc(&dsn, "SELEKT 1", true).await;

            assert!(matches!(result, Err(DbOperationError::QueryFailed(message))
                    if message.to_ascii_lowercase().contains("syntax error")));
        }

        #[tokio::test]
        async fn foreign_key_cascade_applies_to_parent_delete() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
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
                .execute_write(&dsn, "DELETE FROM orgs WHERE id = 1", false)
                .await
                .unwrap();
            let children = adapter
                .execute_adhoc(&dsn, "SELECT id FROM users", true)
                .await
                .unwrap();

            assert_eq!(result.affected_rows, 1);
            assert!(children.rows().is_empty());
        }

        #[tokio::test]
        async fn returns_affected_rows() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "DELETE FROM users WHERE id IN (1, 2)", false)
                .await
                .unwrap();

            assert_eq!(result.affected_rows, 2);
        }

        #[tokio::test]
        async fn count_query_rows_parses_count_result() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY);
            INSERT INTO users(id) VALUES (1), (2), (3);
            ",
            );
            let adapter = SqliteAdapter::new();

            let count = adapter
                .count_query_rows(&dsn, "SELECT COUNT(*) FROM users", true)
                .await
                .unwrap();

            assert_eq!(count, 3);
        }

        #[tokio::test]
        async fn export_to_csv_writes_rows_and_returns_row_count() {
            let (dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO users(id, name) VALUES (1, 'a'), (2, 'b');
            ",
            );
            let path = dir.path().join("users.csv");
            let adapter = SqliteAdapter::new();

            let row_count = adapter
                .export_to_csv(&dsn, "SELECT id, name FROM users ORDER BY id", &path, true)
                .await
                .unwrap();
            let csv = std::fs::read_to_string(path).unwrap();

            assert_eq!(row_count, 2);
            assert_eq!(csv, "id,name\n1,a\n2,b\n");
        }

        #[tokio::test]
        async fn export_to_csv_counts_records_with_embedded_newlines() {
            let (dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                r"
            CREATE TABLE logs(id INTEGER PRIMARY KEY, message TEXT);
            INSERT INTO logs(id, message) VALUES (1, 'hello
world'), (2, 'done');
            ",
            );
            let path = dir.path().join("logs.csv");
            let adapter = SqliteAdapter::new();

            let row_count = adapter
                .export_to_csv(
                    &dsn,
                    "SELECT id, message FROM logs ORDER BY id",
                    &path,
                    true,
                )
                .await
                .unwrap();

            assert_eq!(row_count, 2);
        }

        #[tokio::test]
        async fn export_to_csv_rejects_write_sql() {
            let (dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let path = dir.path().join("write_export.csv");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .export_to_csv(&dsn, "INSERT INTO users(id) VALUES (1)", &path, false)
                .await;

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(message))
                if message.contains("write or DDL")
            ));
            assert!(!path.exists());
        }

        #[tokio::test]
        async fn export_to_csv_missing_table_returns_object_missing_and_removes_file() {
            let (dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let path = dir.path().join("missing_export.csv");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .export_to_csv(&dsn, "SELECT id FROM missing", &path, true)
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
            assert!(!path.exists());
        }

        #[tokio::test]
        async fn count_query_rows_missing_table_returns_object_missing() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db("");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .count_query_rows(&dsn, "SELECT COUNT(*) FROM missing", true)
                .await;

            assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        }

        #[tokio::test]
        async fn read_only_write_fails() {
            let (_dir, dsn) = sabiql_test_support::infra::make_sqlite_db(
                "CREATE TABLE users(id INTEGER PRIMARY KEY);",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_write(&dsn, "INSERT INTO users(id) VALUES (1)", true)
                .await;

            assert!(matches!(result, Err(DbOperationError::PermissionDenied(_))));
        }
    }

    mod dsn_validation {
        use super::*;

        #[tokio::test]
        async fn relative_path_starting_with_dash_is_opened_as_database_path() {
            struct CleanupPath(String);
            impl Drop for CleanupPath {
                fn drop(&mut self) {
                    let _ = std::fs::remove_file(&self.0);
                }
            }

            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = format!("-sabiql-{unique}.db");
            let _cleanup = CleanupPath(path.clone());
            let dsn = format!("sqlite://{path}");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS value", false)
                .await;

            let result = result.unwrap();
            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
        }
    }
}
