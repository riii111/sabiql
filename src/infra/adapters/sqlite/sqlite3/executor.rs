use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::outbound::DbOperationError;

use super::super::path_validation;
use super::error::{classify_cli_spawn_error, classify_query_error};
const BUSY_TIMEOUT_MS: u64 = 5_000;

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

    pub(in crate::adapters::sqlite) async fn execute_json_read_only<T: DeserializeOwned>(
        &self,
        path: &str,
        sql: &str,
    ) -> Result<T, DbOperationError> {
        let stdout = self.execute_text(path, &["-json"], sql, true).await?;
        let stdout = match stdout.trim() {
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
        self.execute_text(
            path,
            &["-batch", "-bail", "-csv", "-header"],
            sql,
            read_only,
        )
        .await
    }

    pub(in crate::adapters::sqlite) async fn execute_quote(
        &self,
        path: &str,
        sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        self.execute_text(
            path,
            &["-batch", "-bail", "-quote", "-header"],
            sql,
            read_only,
        )
        .await
    }

    async fn execute_text(
        &self,
        path: &str,
        args: &[&str],
        sql: &str,
        read_only: bool,
    ) -> Result<String, DbOperationError> {
        let output = self.run(path, args, sql, read_only).await?;
        if !output.status.success() {
            return Err(classify_query_error(&output.stderr));
        }
        Ok(output.stdout)
    }

    async fn run(
        &self,
        path: &str,
        args: &[&str],
        sql: &str,
        read_only: bool,
    ) -> Result<SqliteOutput, DbOperationError> {
        let mut cmd = Self::build_command("sqlite3", path, args, read_only)?;
        let sql = sqlite_session_sql(sql, read_only);
        #[cfg(test)]
        super::tests::record_process_start(path);
        Self::collect_output(&mut cmd, self.timeout_secs, &sql).await
    }

    fn build_command(
        program: &str,
        path: &str,
        args: &[&str],
        read_only: bool,
    ) -> Result<Command, DbOperationError> {
        Self::ensure_database_path(path)?;
        let mut cmd = Command::new(program);
        #[cfg(test)]
        super::tests::configure_command(path, &mut cmd);
        Self::apply_session_options(&mut cmd, read_only);
        cmd.args(args).arg(sqlite_database_uri(path, read_only));
        Ok(cmd)
    }

    fn ensure_database_path(path: &str) -> Result<(), DbOperationError> {
        path_validation::validate_sqlite_database_path(Path::new(path))
            .map_err(DbOperationError::SqlitePath)
    }

    fn apply_session_options(cmd: &mut Command, read_only: bool) {
        Self::apply_initialization_file(cmd);
        cmd.arg("--safe");
        if read_only {
            cmd.arg("-readonly");
        }
        cmd.arg("-cmd").arg(format!(".timeout {BUSY_TIMEOUT_MS}"));
        cmd.arg("-cmd").arg(".explain off");
    }

    fn apply_initialization_file(cmd: &mut Command) {
        cmd.arg("-init").arg(sqlite_empty_init_file());
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
            .map_err(classify_cli_spawn_error)?;

        let stdin = child.stdin.take().expect("piped sqlite stdin");
        let mut stdout_handle = child.stdout.take().expect("piped sqlite stdout");
        let mut stderr_handle = child.stderr.take().expect("piped sqlite stderr");

        let result = timeout(Duration::from_secs(timeout_secs), async {
            let (stdin_result, stdout_result, stderr_result) = tokio::join!(
                write_sql_to_stdin(stdin, sql),
                async {
                    let mut buf = Vec::new();
                    stdout_handle.read_to_end(&mut buf).await?;
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                },
                async {
                    let mut buf = Vec::new();
                    stderr_handle.read_to_end(&mut buf).await?;
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                }
            );

            stdin_result?;
            let stdout = stdout_result?;
            let stderr = stderr_result?;
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stdout, stderr))
        })
        .await;

        let result = match result {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                kill_and_wait(&mut child).await;
                return Err(DbOperationError::QueryFailed(error.to_string()));
            }
            Err(error) => {
                kill_and_wait(&mut child).await;
                return Err(DbOperationError::Timeout(error.to_string()));
            }
        };

        let (status, stdout, stderr) = result;
        Ok(SqliteOutput {
            status,
            stdout,
            stderr,
        })
    }
}

async fn kill_and_wait(child: &mut tokio::process::Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

async fn write_sql_to_stdin(
    mut stdin: tokio::process::ChildStdin,
    sql: &str,
) -> Result<(), std::io::Error> {
    let execution_sql = terminated_sql(sql);
    if let Err(error) = stdin.write_all(execution_sql.as_bytes()).await
        && error.kind() != std::io::ErrorKind::BrokenPipe
    {
        return Err(error);
    }
    if let Err(error) = stdin.shutdown().await
        && error.kind() != std::io::ErrorKind::BrokenPipe
    {
        return Err(error);
    }
    Ok(())
}

fn sqlite_empty_init_file() -> &'static str {
    sqlite_empty_init_file_for_platform(cfg!(windows))
}

const fn sqlite_empty_init_file_for_platform(is_windows: bool) -> &'static str {
    if is_windows { "NUL" } else { "/dev/null" }
}

fn terminated_sql(sql: &str) -> String {
    format!("{sql}\n;\n")
}

fn sqlite_session_sql(sql: &str, read_only: bool) -> String {
    let query_only = if read_only {
        "PRAGMA query_only=ON;\n"
    } else {
        ""
    };
    format!("PRAGMA foreign_keys=ON;\n{query_only}{sql}")
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use crate::adapters::sqlite::SqliteAdapter;
    use crate::adapters::test_support;
    use crate::app::ports::outbound::AccessMode;
    use crate::domain::connection::SqliteConnectionConfig;

    use super::*;

    #[cfg(unix)]
    mod process_lifecycle {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        use tempfile::TempDir;

        use super::*;

        fn fake_sqlite() -> (TempDir, PathBuf, PathBuf) {
            let directory = tempfile::tempdir().unwrap();
            let program = directory.path().join("sqlite3");
            let pid_file = directory.path().join("pid");
            let script = r#"#!/bin/sh
printf '%s\n' "$$" > "$SABIQL_FAKE_SQLITE_PID"
case "$SABIQL_FAKE_SQLITE_MODE" in
  normal)
    printf 'value\n1\n'
    ;;
  error)
    printf 'fake sqlite error\n' >&2
    exit 1
    ;;
  timeout)
    while :; do :; done
    ;;
esac
"#;
            fs::write(&program, script).unwrap();
            let mut permissions = fs::metadata(&program).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&program, permissions).unwrap();
            (directory, program, pid_file)
        }

        fn assert_process_reaped(pid_file: &Path) {
            let mut pid = None;
            for _ in 0..200 {
                if let Ok(value) = fs::read_to_string(pid_file)
                    && let Ok(value) = value.trim().parse::<libc::pid_t>()
                {
                    pid = Some(value);
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let Some(pid) = pid else {
                panic!("fake sqlite process did not start: {}", pid_file.display());
            };

            for _ in 0..200 {
                if unsafe { libc::kill(pid, 0) } == -1 {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("fake sqlite process {pid} is still running or unreaped");
        }

        fn command_environment(
            directory: &TempDir,
            mode: &str,
            pid_file: &Path,
        ) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
            vec![
                (
                    std::ffi::OsString::from("PATH"),
                    directory.path().as_os_str().to_owned(),
                ),
                (
                    std::ffi::OsString::from("SABIQL_FAKE_SQLITE_MODE"),
                    std::ffi::OsString::from(mode),
                ),
                (
                    std::ffi::OsString::from("SABIQL_FAKE_SQLITE_PID"),
                    pid_file.as_os_str().to_owned(),
                ),
            ]
        }

        #[tokio::test]
        async fn collect_reaps_fake_sqlite_before_returning() {
            for mode in ["normal", "error", "timeout"] {
                let (_database_dir, dsn) = test_support::make_sqlite_db("");
                let (fake_dir, _program, pid_file) = fake_sqlite();
                let (mut adapter, _command_context) = SqliteAdapter::with_test_environment(
                    &dsn,
                    command_environment(&fake_dir, mode, &pid_file),
                );
                adapter.cli.timeout_secs = if mode == "timeout" { 1 } else { 30 };

                let result = adapter.cli.execute_csv(dsn.path(), "SELECT 1", true).await;

                match mode {
                    "normal" => assert_eq!(result.unwrap(), "value\n1\n"),
                    "error" => assert!(
                        matches!(
                            &result,
                            Err(DbOperationError::QueryFailed(details))
                                if details == "fake sqlite error"
                        ),
                        "result={result:?}"
                    ),
                    "timeout" => assert!(matches!(result, Err(DbOperationError::Timeout(_)))),
                    _ => unreachable!(),
                }
                assert_process_reaped(&pid_file);
            }
        }
    }

    mod dsn_validation {
        use super::*;

        #[test]
        fn empty_initialization_file_uses_platform_null_device() {
            assert_eq!(sqlite_empty_init_file_for_platform(false), "/dev/null");
            assert_eq!(sqlite_empty_init_file_for_platform(true), "NUL");
        }

        #[test]
        fn session_options_include_safety_timeout_and_explain_output() {
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
                    ".explain off",
                ]
            );
        }

        #[test]
        fn session_pragmas_precede_user_sql() {
            assert_eq!(
                sqlite_session_sql("SELECT 1", true),
                "PRAGMA foreign_keys=ON;\nPRAGMA query_only=ON;\nSELECT 1"
            );
            assert_eq!(
                sqlite_session_sql("SELECT 1", false),
                "PRAGMA foreign_keys=ON;\nSELECT 1"
            );
        }

        #[test]
        fn terminates_sql_with_a_standalone_statement_separator() {
            assert_eq!(
                terminated_sql("SELECT 1 -- trailing comment"),
                "SELECT 1 -- trailing comment\n;\n"
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
            let dsn = SqliteConnectionConfig::new(path).unwrap();
            let adapter = SqliteAdapter::new();

            let result = adapter
                .execute_adhoc(&dsn, "SELECT 1 AS value", AccessMode::ReadWrite)
                .await;

            let result = result.unwrap();
            assert_eq!(test_support::display_row(&result, 0), vec!["1".to_string()]);
        }

        #[cfg(not(windows))]
        mod initialization_isolation {
            use crate::adapters::sqlite::sqlite3::tests::TestCommandContext;

            use super::*;

            struct InitializationArtifacts {
                redirected_database: PathBuf,
                redirected_output: PathBuf,
                _command_context: TestCommandContext,
            }

            fn adapter_with_malicious_initialization(
                dir: &tempfile::TempDir,
                dsn: &SqliteConnectionConfig,
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

                let (adapter, command_context) = SqliteAdapter::with_test_environment(
                    dsn,
                    vec![
                        (OsString::from("HOME"), home.into_os_string()),
                        (
                            OsString::from("XDG_CONFIG_HOME"),
                            xdg_config_home.into_os_string(),
                        ),
                    ],
                );
                (
                    adapter,
                    InitializationArtifacts {
                        redirected_database,
                        redirected_output,
                        _command_context: command_context,
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
                let (adapter, artifacts) = adapter_with_malicious_initialization(&dir, &dsn);

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
                let diagnostics = adapter.fetch_core_diagnostics(&dsn).await.unwrap();

                assert_eq!(write.affected_rows, 1);
                assert_eq!(
                    test_support::display_row(&result, 0),
                    vec!["Grace".to_string()]
                );
                assert_eq!(metadata.table_summaries.len(), 1);
                assert_eq!(
                    (
                        test_support::display_row(&preview, 0),
                        test_support::display_row(&preview, 1),
                    ),
                    (
                        vec!["1".to_string(), "Ada".to_string()],
                        vec!["2".to_string(), "Grace".to_string()]
                    )
                );
                assert!(diagnostics.sqlite_version.is_ok());
                assert_initialization_was_not_loaded(&artifacts);
            }
        }
    }
}
