use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use async_trait::async_trait;

use crate::adapters::csv_export::export_to_downloads;
use crate::app::policy::sql::sqlite_explain::is_sqlite_explain_query_plan_sql;
use crate::app::ports::outbound::{
    AccessMode, DatabaseCli, DbOperationError, QueryExecutor, SQLITE_SAFE_MODE_REQUIRED_MARKER,
};
use crate::domain::{
    CommandTag, QueryResult, QuerySource, TableKind, WriteExecutionResult,
    available_sqlite_rowid_alias,
};

use super::super::{SqliteAdapter, path_validation, sql};
use super::error::{classify_cli_spawn_error, classify_query_error};
use super::parser::{
    SqliteStatementPlan, aggregate_sqlite_command_tag, append_changes_query_for_plan,
    command_tag_result, is_sqlite_rerunnable_export_query, last_sqlite_result_set,
    parse_affected_rows, parse_count_result, quoted_to_query_result,
    sqlite_adhoc_execution_query_for_plan, sqlite_export_not_rerunnable_error, sqlite_probe_marker,
    sqlite_statement_plan, sqlite_statement_tags, statement_counts_as_select_tag,
    strip_sqlite_probes,
};

pub(in crate::adapters::sqlite) const BUSY_TIMEOUT_MS: u64 = 5_000;

const SQLITE_SAFE_MODE_MIN_VERSION: SqliteVersion = SqliteVersion::new(3, 41, 1);

#[derive(Debug, Clone)]
pub(in crate::adapters::sqlite) struct SqliteCli {
    timeout_secs: u64,
    #[cfg(test)]
    environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
}

struct SqliteOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SqliteVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl SqliteVersion {
    const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    fn parse(output: &str) -> Option<Self> {
        let mut components = output.split_whitespace().next()?.split('.');
        let major = components.next()?.parse().ok()?;
        let minor = components.next()?.parse().ok()?;
        let patch = components.next()?.parse().ok()?;
        components
            .next()
            .is_none()
            .then_some(Self::new(major, minor, patch))
    }
}

impl SqliteCli {
    pub(in crate::adapters::sqlite) fn new() -> Self {
        Self {
            timeout_secs: 30,
            #[cfg(test)]
            environment: Vec::new(),
        }
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

    pub(in crate::adapters::sqlite) async fn ensure_safe_mode_supported(
        &self,
    ) -> Result<(), DbOperationError> {
        let mut cmd = self.command();
        Self::apply_initialization_file(&mut cmd);
        let output = cmd
            .arg("--safe")
            .arg("--version")
            .output()
            .await
            .map_err(|error| classify_cli_spawn_error(DatabaseCli::Sqlite3, error))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = SqliteVersion::parse(&stdout).ok_or_else(|| {
            safe_mode_required_error("could not determine the installed sqlite3 version")
        })?;

        if output.status.success() && version >= SQLITE_SAFE_MODE_MIN_VERSION {
            return Ok(());
        }

        let details = if output.status.success() {
            format!(
                "found sqlite3 {}.{}.{}",
                version.major, version.minor, version.patch
            )
        } else {
            "sqlite3 --version failed".to_string()
        };
        Err(safe_mode_required_error(&details))
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

    pub(in crate::adapters::sqlite) async fn execute_quote_with_explain_off(
        &self,
        path: &str,
        sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        let output = self
            .run(
                path,
                &[
                    "-batch",
                    "-bail",
                    "-quote",
                    "-header",
                    "-cmd",
                    ".explain off",
                ],
                sql,
                read_only,
            )
            .await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        Ok(output.stdout)
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
            self.execute_quote_with_explain_off(path, execution_sql, read_only)
                .await
        } else {
            self.execute_quote(path, execution_sql, read_only).await
        }
    }

    pub(in crate::adapters::sqlite) async fn export_csv(
        &self,
        path: &str,
        sql: &str,
        output_path: &std::path::Path,
        read_only: bool,
    ) -> Result<(), DbOperationError> {
        self.export_csv_with_command("sqlite3", path, sql, output_path, read_only)
            .await
    }

    async fn export_csv_with_command(
        &self,
        command: &str,
        path: &str,
        sql: &str,
        output_path: &std::path::Path,
        read_only: bool,
    ) -> Result<(), DbOperationError> {
        Self::ensure_database_path(path)?;
        let mut cmd = self.command_with_program(command);
        Self::apply_session_options(&mut cmd, read_only);
        cmd.arg("-batch").arg("-bail").arg("-csv").arg("-header");
        cmd.arg(sqlite_database_uri(path, read_only));

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| classify_cli_spawn_error(DatabaseCli::Sqlite3, error))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let file = tokio::fs::File::create(output_path)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let mut writer = tokio::io::BufWriter::new(file);

        let result = timeout(Duration::from_secs(self.timeout_secs * 10), async {
            let (stdin_result, stdout_result, stderr_result) = tokio::join!(
                write_sql_to_stdin(stdin, sql),
                async {
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
                    Ok::<_, std::io::Error>(())
                },
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut stderr) = stderr_handle {
                        stderr.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                }
            );

            stdin_result?;
            stdout_result?;
            let stderr = stderr_result?;
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

        Ok(())
    }

    async fn run(
        &self,
        path: &str,
        args: &[&str],
        sql: &str,
        read_only: bool,
    ) -> Result<SqliteOutput, DbOperationError> {
        Self::ensure_database_path(path)?;
        let mut cmd = self.command();
        Self::apply_session_options(&mut cmd, read_only);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.arg(sqlite_database_uri(path, read_only));
        Self::collect_output(&mut cmd, self.timeout_secs, sql).await
    }

    fn ensure_database_path(path: &str) -> Result<(), DbOperationError> {
        path_validation::validate_sqlite_database_path(Path::new(path))
            .map_err(|error| DbOperationError::ConnectionFailed(error.to_string()))
    }

    fn apply_session_options(cmd: &mut Command, read_only: bool) {
        Self::apply_initialization_file(cmd);
        cmd.arg("--safe");
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

    fn apply_initialization_file(cmd: &mut Command) {
        cmd.arg("-init").arg(sqlite_empty_init_file());
    }

    fn command(&self) -> Command {
        self.command_with_program("sqlite3")
    }

    fn command_with_program(&self, program: &str) -> Command {
        #[cfg(test)]
        let command = {
            let mut cmd = Command::new(program);
            for (key, value) in &self.environment {
                cmd.env(key, value);
            }
            cmd
        };
        #[cfg(not(test))]
        let command = Command::new(program);
        command
    }

    async fn collect_output(
        cmd: &mut Command,
        timeout_secs: u64,
        sql: &str,
    ) -> Result<SqliteOutput, DbOperationError> {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| classify_cli_spawn_error(DatabaseCli::Sqlite3, error))?;

        let stdin = child.stdin.take();
        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let result = timeout(Duration::from_secs(timeout_secs), async {
            let (stdin_result, stdout_result, stderr_result) = tokio::join!(
                write_sql_to_stdin(stdin, sql),
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

            stdin_result?;
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

fn safe_mode_required_error(details: &str) -> DbOperationError {
    DbOperationError::UnsupportedOperation(format!(
        "{SQLITE_SAFE_MODE_REQUIRED_MARKER}: sqlite3 3.41.1 or later is required for safe SQLite execution ({details})"
    ))
}

fn sqlite_empty_init_file() -> &'static str {
    sqlite_empty_init_file_for_platform(cfg!(windows))
}

const fn sqlite_empty_init_file_for_platform(is_windows: bool) -> &'static str {
    if is_windows { "NUL" } else { "/dev/null" }
}

async fn write_sql_to_stdin(
    stdin: Option<tokio::process::ChildStdin>,
    sql: &str,
) -> Result<(), std::io::Error> {
    if let Some(mut stdin) = stdin {
        if let Err(error) = stdin.write_all(sql.as_bytes()).await
            && error.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(error);
        }
        if let Err(error) = stdin.shutdown().await
            && error.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(error);
        }
    }
    Ok(())
}

fn sqlite_database_uri(path: &str, read_only: bool) -> String {
    sqlite_database_uri_for_platform(path, read_only, cfg!(windows))
}

fn sqlite_database_uri_for_platform(path: &str, read_only: bool, is_windows: bool) -> String {
    let mode = if read_only { "ro" } else { "rw" };
    let path = sqlite_uri_path(path, is_windows);
    format!("file:{}?mode={mode}", urlencoding::encode(&path))
}

fn sqlite_uri_path(path: &str, is_windows: bool) -> String {
    if !is_windows {
        return path.to_string();
    }

    let path = path.replace('\\', "/");
    if path.as_bytes().get(1) == Some(&b':') && !path.starts_with('/') {
        format!("/{path}")
    } else {
        path
    }
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
        plan: &SqliteStatementPlan<'_>,
        read_only: bool,
    ) -> Result<(usize, u64), DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures sqlite3 execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let stdout = self
            .cli
            .execute_csv(path, &append_changes_query_for_plan(plan), read_only)
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
            .execute_quoted_query(path, &query, QuerySource::Preview, true)
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
            .cli
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
        let (affected_rows, execution_time_ms) = self
            .execute_changes_query(path, &plan, access_mode.is_read_only())
            .await?;
        Ok(WriteExecutionResult {
            affected_rows,
            execution_time_ms,
        })
    }

    async fn count_query_rows(&self, dsn: &str, query: &str) -> Result<usize, DbOperationError> {
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

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use crate::adapters::csv_export::export_to_path;
    use crate::app::ports::outbound::{AccessMode, SqlDialect};
    use crate::domain::{
        CommandTag, DatabaseType, QuerySource, QueryValue,
        sqlite_explain_query_plan_text_from_result,
    };

    use super::*;

    impl SqliteCli {
        fn with_environment(mut self, environment: Vec<(OsString, OsString)>) -> Self {
            self.environment = environment;
            self
        }
    }

    mod preview {
        use crate::adapters::test_support;

        use super::*;

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
            assert_eq!(result.rows(), vec![vec!["2".to_string(), "b".to_string()]]);
        }

        #[tokio::test]
        async fn rowid_table_preview_hides_rowid_but_keeps_internal_identity() {
            let (_dir, dsn) = test_support::make_sqlite_db(
                r"
            CREATE TABLE logs(message TEXT);
            INSERT INTO logs(message) VALUES ('first'), ('second');
            ",
            );
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0)
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
            let (_dir, dsn) = test_support::make_sqlite_db(
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
            let write = adapter
                .execute_write(&dsn, &sql, AccessMode::ReadWrite)
                .await
                .unwrap();
            let preview = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0)
                .await
                .unwrap();

            assert_eq!(write.affected_rows, 1);
            assert_eq!(preview.rows(), vec![vec!["new".to_string()]]);
        }

        #[tokio::test]
        async fn rowid_update_predicate_rejects_reused_rowid_with_changed_values() {
            let (_dir, dsn) = test_support::make_sqlite_db(
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
                .execute_write(
                    &dsn,
                    "DELETE FROM logs WHERE rowid = 1",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO logs(message, note) VALUES ('replacement', NULL)",
                    AccessMode::ReadWrite,
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
            let write = adapter
                .execute_write(&dsn, &sql, AccessMode::ReadWrite)
                .await
                .unwrap();
            let remaining = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0)
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
            let (_dir, dsn) = test_support::make_sqlite_db(
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
            let write = adapter
                .execute_write(&dsn, &sql, AccessMode::ReadWrite)
                .await
                .unwrap();
            let preview = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0)
                .await
                .unwrap();

            assert_eq!(write.affected_rows, 1);
            assert_eq!(preview.row_count(), 0);
        }

        #[tokio::test]
        async fn rowid_delete_predicate_rejects_reused_rowid_with_changed_values() {
            let (_dir, dsn) = test_support::make_sqlite_db(
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
                .execute_write(
                    &dsn,
                    "DELETE FROM logs WHERE rowid = 1",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            adapter
                .execute_write(
                    &dsn,
                    "INSERT INTO logs(message) VALUES ('replacement')",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();

            let sql =
                adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "logs", &stale_rows);
            let write = adapter
                .execute_write(&dsn, &sql, AccessMode::ReadWrite)
                .await
                .unwrap();
            let remaining = adapter
                .execute_preview(&dsn, "main", "logs", 10, 0)
                .await
                .unwrap();

            assert_eq!(write.affected_rows, 0);
            assert_eq!(remaining.row_count(), 1);
            assert_eq!(remaining.rows(), vec![vec!["replacement".to_string()]]);
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
                assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
                assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
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
                assert_eq!(rows.rows(), vec![vec!["2".to_string()]]);
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
                    result.rows(),
                    vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
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
                    result.rows(),
                    vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
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
                assert_eq!(result.rows(), vec![vec!["5".to_string(), "6".to_string()]]);
                assert_eq!(result.command_tag, Some(CommandTag::Select(1)));
            }

            #[tokio::test]
            async fn multi_select_empty_trailing_result_returns_empty_result() {
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

                assert!(result.columns.is_empty());
                assert!(result.rows().is_empty());
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

                assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
            }

            #[tokio::test]
            async fn read_only_session_enables_query_only_before_user_sql() {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(&dsn, "PRAGMA query_only", AccessMode::ReadOnly)
                    .await
                    .unwrap();

                assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
            }

            #[tokio::test]
            async fn applies_busy_timeout_before_user_sql() {
                let (_dir, dsn) = test_support::make_sqlite_db("");
                let adapter = SqliteAdapter::new();

                let result = adapter
                    .execute_adhoc(&dsn, "PRAGMA busy_timeout", AccessMode::ReadOnly)
                    .await
                    .unwrap();

                assert_eq!(result.rows(), vec![vec![BUSY_TIMEOUT_MS.to_string()]]);
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

                assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
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
                assert_eq!(result.rows(), vec![vec!["x".to_string()]]);
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
                    assert!(tables.rows().is_empty());
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

                    assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
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

                    assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
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
                    assert!(rows.rows().is_empty());
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
                    assert!(rows.rows().is_empty());
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
                    assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
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
                    assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
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
                    assert_eq!(rows.rows(), vec![vec!["1".to_string()]]);
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
                    assert_eq!(rows.rows(), vec![vec!["3".to_string()]]);
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
                    assert!(rows.rows().is_empty());
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
                    assert!(rows.rows().is_empty());
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
                    assert!(rows.rows().is_empty());
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
                    assert!(rows.rows().is_empty());
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
                    assert!(rows.rows().is_empty());
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
                    assert!(rows.rows().is_empty());
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
                assert_eq!(result.rows(), vec![vec!["1".to_string(), "a".to_string()]]);
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

                assert_eq!(result.rows().len(), 2);
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

                assert_eq!(result.rows(), vec![vec!["1".to_string(), "a".to_string()]]);
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
            assert_eq!(children.rows(), vec![vec!["1".to_string()]]);
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
            assert!(children.rows().is_empty());
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
        async fn export_to_csv_spawn_failure_leaves_no_output_files() {
            let (dir, dsn) =
                test_support::make_sqlite_db("CREATE TABLE users(id INTEGER PRIMARY KEY);");
            let final_path = dir.path().join("export.csv");
            let adapter = SqliteAdapter::new();

            let result = export_to_path(final_path.clone(), |temporary_path| async move {
                adapter
                    .cli
                    .export_csv_with_command(
                        "sabiql-missing-sqlite3",
                        SqliteAdapter::path_from_dsn(&dsn)?,
                        "SELECT id FROM users",
                        &temporary_path,
                        true,
                    )
                    .await
            })
            .await;

            assert!(matches!(
                result,
                Err(DbOperationError::CommandNotFound { .. })
            ));
            assert!(!final_path.exists());
            assert!(!dir.path().read_dir().unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".export.csv")
            }));
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

    mod dsn_validation {
        use super::*;

        #[rstest::rstest]
        #[case("3.41.1 2023-03-10 12:13:52", Some(SqliteVersion::new(3, 41, 1)))]
        #[case("3.40.1", Some(SqliteVersion::new(3, 40, 1)))]
        #[case("3.41", None)]
        #[case("sqlite 3.41.1", None)]
        fn parses_sqlite_cli_version(
            #[case] output: &str,
            #[case] expected: Option<SqliteVersion>,
        ) {
            assert_eq!(SqliteVersion::parse(output), expected);
        }

        #[test]
        fn safe_mode_requires_sqlite_3_41_1_or_later() {
            assert!(SqliteVersion::new(3, 41, 0) < SQLITE_SAFE_MODE_MIN_VERSION);
            assert!(SqliteVersion::new(3, 41, 1) >= SQLITE_SAFE_MODE_MIN_VERSION);
        }

        #[test]
        fn empty_initialization_file_uses_platform_null_device() {
            assert_eq!(sqlite_empty_init_file_for_platform(false), "/dev/null");
            assert_eq!(sqlite_empty_init_file_for_platform(true), "NUL");
        }

        #[test]
        fn initialization_precedes_safe_read_only_and_session_options() {
            let mut cmd = Command::new("sqlite3");
            SqliteCli::apply_session_options(&mut cmd, true);
            let args = cmd
                .as_std()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();

            assert_eq!(
                args,
                vec![
                    "-init",
                    sqlite_empty_init_file(),
                    "--safe",
                    "-readonly",
                    "-cmd",
                    &format!(".timeout {BUSY_TIMEOUT_MS}"),
                    "-cmd",
                    "PRAGMA foreign_keys=ON",
                    "-cmd",
                    "PRAGMA query_only=ON",
                ]
            );
        }

        #[test]
        fn database_uri_uses_non_creating_access_modes() {
            let read_write = sqlite_database_uri("/tmp/sabiql database?.db", false);
            let read_only = sqlite_database_uri("/tmp/sabiql database?.db", true);

            assert!(read_write.starts_with("file:"));
            assert!(read_write.contains("%3F"));
            assert!(read_write.ends_with("?mode=rw"));
            assert!(read_only.ends_with("?mode=ro"));
        }

        #[test]
        fn windows_database_uri_normalizes_drive_paths() {
            assert_eq!(
                sqlite_uri_path(r"C:\Users\sabiql\database.sqlite", true),
                "/C:/Users/sabiql/database.sqlite"
            );
            assert!(
                sqlite_database_uri_for_platform(r"C:\Users\sabiql\database.sqlite", false, true,)
                    .starts_with("file:%2FC%3A%2FUsers%2Fsabiql%2Fdatabase.sqlite?")
            );
        }

        #[tokio::test]
        async fn read_write_uri_rejects_missing_database_without_creating_file() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("missing.db");
            let mut child = Command::new("sqlite3")
                .arg(sqlite_database_uri(path.to_str().unwrap(), false))
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let mut stdin = child.stdin.take().unwrap();
            stdin
                .write_all(b"CREATE TABLE users(id INTEGER)")
                .await
                .unwrap();
            stdin.shutdown().await.unwrap();
            drop(stdin);
            let output = child.wait_with_output().await.unwrap();

            assert!(!output.status.success());
            assert!(!path.exists());
        }

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
            std::fs::write(&path, b"").unwrap();
            let dsn = format!("sqlite://{path}");
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS value", AccessMode::ReadWrite)
                .await;

            let result = result.unwrap();
            assert_eq!(result.rows(), vec![vec!["1".to_string()]]);
        }

        #[cfg(not(windows))]
        mod initialization_isolation {
            use crate::adapters::test_support;
            use crate::app::ports::outbound::{MetadataProvider, SqliteDiagnosticsProvider};

            use super::*;

            struct InitializationArtifacts {
                redirected_database: PathBuf,
                redirected_output: PathBuf,
            }

            fn adapter_with_malicious_initialization(
                dir: &tempfile::TempDir,
            ) -> (SqliteAdapter, InitializationArtifacts) {
                let home = dir.path().join("home");
                let xdg_config_home = dir.path().join("xdg-config");
                let redirected_database = dir.path().join("redirected.sqlite");
                let redirected_output = dir.path().join("redirected.csv");
                std::fs::create_dir_all(&home).unwrap();
                std::fs::create_dir_all(&xdg_config_home).unwrap();
                std::fs::write(
                    home.join(".sqliterc"),
                    format!(
                        ".output {}\n.mode csv\n.open {}\nCREATE TABLE initialization_side_effect(value TEXT);\n.exit\n",
                        redirected_output.display(),
                        redirected_database.display(),
                    ),
                )
                .unwrap();

                let mut adapter = SqliteAdapter::new();
                adapter.cli = SqliteCli::new().with_environment(vec![
                    (OsString::from("HOME"), home.into_os_string()),
                    (
                        OsString::from("XDG_CONFIG_HOME"),
                        xdg_config_home.into_os_string(),
                    ),
                ]);
                (
                    adapter,
                    InitializationArtifacts {
                        redirected_database,
                        redirected_output,
                    },
                )
            }

            fn assert_initialization_was_not_loaded(artifacts: &InitializationArtifacts) {
                assert!(!artifacts.redirected_database.exists());
                assert!(!artifacts.redirected_output.exists());
            }

            #[tokio::test]
            async fn public_adapter_operations_preserve_query_metadata_preview_and_diagnostics() {
                let (dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT); INSERT INTO users VALUES (1, 'Ada');",
                );
                let (adapter, artifacts) = adapter_with_malicious_initialization(&dir);

                let write = adapter
                    .execute_write(
                        &dsn,
                        "INSERT INTO users VALUES (2, 'Grace')",
                        AccessMode::ReadWrite,
                    )
                    .await
                    .unwrap();
                let result = adapter
                    .execute_adhoc(
                        &dsn,
                        "SELECT name FROM users WHERE id = 2",
                        AccessMode::ReadOnly,
                    )
                    .await
                    .unwrap();
                let metadata = adapter.fetch_metadata(&dsn).await.unwrap();
                let preview = adapter
                    .execute_preview(&dsn, "main", "users", 10, 0)
                    .await
                    .unwrap();
                let diagnostics = adapter.fetch_diagnostics_core(&dsn).await.unwrap();

                assert_eq!(write.affected_rows, 1);
                assert_eq!(result.rows(), vec![vec!["Grace".to_string()]]);
                assert_eq!(metadata.table_summaries.len(), 1);
                assert_eq!(
                    preview.rows(),
                    vec![
                        vec!["1".to_string(), "Ada".to_string()],
                        vec!["2".to_string(), "Grace".to_string()]
                    ]
                );
                assert!(diagnostics.sqlite_version.is_ok());
                assert_initialization_was_not_loaded(&artifacts);
            }

            #[tokio::test]
            async fn export_preserves_csv_protocol() {
                let (dir, dsn) = test_support::make_sqlite_db(
                    "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT); INSERT INTO users VALUES (1, 'Ada');",
                );
                let export_path = dir.path().join("users.csv");
                let (adapter, artifacts) = adapter_with_malicious_initialization(&dir);

                adapter
                    .cli
                    .export_csv(
                        SqliteAdapter::path_from_dsn(&dsn).unwrap(),
                        "SELECT id, name FROM users",
                        &export_path,
                        true,
                    )
                    .await
                    .unwrap();

                assert_eq!(
                    std::fs::read_to_string(export_path).unwrap(),
                    "id,name\n1,Ada\n"
                );
                assert_initialization_was_not_loaded(&artifacts);
            }
        }
    }
}
