use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tempfile::TempDir;

use super::super::export::run_mysql_export_process;
use super::super::policy::MySqlExecutionResult;
use super::super::xml::MySqlResultSet;
use super::adhoc::run_mysql_adhoc_with_program_and_statements;
use super::metadata::{
    mysql_metadata_columns_external_with_program,
    run_mysql_metadata_query_with_read_only_session_with_timeout,
};
use super::single::run_mysql_single_statement_process_with_diagnostics;
use super::*;
use crate::adapters::csv_export::export_to_path;
use crate::domain::mysql_sql::{classify_mysql_statement, split_mysql_statements};
use crate::domain::{CommandTag, DatabaseDiagnostic, DiagnosticLevel, QueryValue, RefreshScope};
async fn export_mysql_csv_with_program(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    path: PathBuf,
    execution_timeout: Duration,
) -> Result<(), DbOperationError> {
    let mut process = MySqlProcess::spawn_with_program(program, option_file)?;
    run_mysql_process_with_timeout(
        execution_timeout,
        &mut process,
        RefreshScope::None,
        async |process| run_mysql_export_process(process, option_file, query, path).await,
    )
    .await
}
async fn run_mysql_single_statement_with_program(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
    execution_timeout: Duration,
) -> Result<MySqlExecutionResult, DbOperationError> {
    let mut process = MySqlProcess::spawn_with_adhoc_program(program, option_file)?;
    run_mysql_process_with_timeout(
        execution_timeout,
        &mut process,
        RefreshScope::None,
        async |process| {
            run_mysql_single_statement_process_with_diagnostics(process, query, access_mode).await
        },
    )
    .await
}

fn fake_mysql(mode: &str) -> (TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let option_file = directory.path().join("option.cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let program = directory.path().join("mysql");
    let log_file = PathBuf::from(format!("{}.log", option_file.display()));
    let session_response = match mode {
            "missing" => "exit 0".to_string(),
            "invalid" => {
                "printf '%s\\n' '<resultset><row><field name=\"wrong\">x</field></row></resultset>'"
                    .to_string()
            }
            "unsupported" => "printf '%s\\n' '<resultset><row><field name=\"__sabiql_session_marker\">'\"$marker\"'</field><field name=\"__sabiql_sql_mode\">ANSI_QUOTES</field></row></resultset>'".to_string(),
            "timeout" => "while :; do :; done".to_string(),
            _ => "printf '%s\\n' '<resultset><row><field name=\"__sabiql_session_marker\">'\"$marker\"'</field><field name=\"__sabiql_sql_mode\">STRICT_TRANS_TABLES</field></row></resultset>'".to_string(),
        };
    let user_response = if mode == "nonzero_exit" {
        "printf '%s\\n' '<resultset><row><field name=\"partial\">row</field></row></resultset>'"
    } else if mode == "failure" {
        "printf '%s\\n' '<resultset><row><field name=\"partial\">row</field></row></resultset>'\n    printf '%s\\n' 'ERROR 1064 (42000): syntax error' >&2\n    exit_status=1"
    } else if mode == "no_result_failure" {
        "printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2\n    exit 1"
    } else if mode == "connection_refused" {
        "printf '%s\\n' \"ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (111)\" >&2\n    exit 1"
    } else if mode == "field_error" {
        "printf '%s\\n' '<resultset><row><field name=\"message\">line 1
ERROR 1146 (42S02): this is a cell value</field></row></resultset>'"
    } else {
        "printf '%s\\n' '<resultset><row><field name=\"value\">ok</field></row></resultset>'"
    };
    let session_failure = if mode == "read_only_failure" {
        "printf '%s\\n' 'ERROR 1227 (42000): access denied to set transaction read only' >&2\n      exit 1"
    } else {
        ""
    };
    let finish_status = if mode == "nonzero_exit" {
        "exit_status=1\n        IFS= read -r marker_terminator\n        dd bs=1 count=1 >/dev/null 2>&1\n        break"
    } else {
        ""
    };
    let settings_timeout = if mode == "timeout" {
        "while :; do :; done"
    } else {
        ""
    };
    let script = format!(
        r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
eof=$(printf '\004')
exit_status=0
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  [ "$line" = "$eof" ] && break
  [ "$line" = ";" ] && continue
  case "$line" in
    "SET SESSION autocommit=1, completion_type=NO_CHAIN")
      {settings_timeout}
      ;;
    "SET SESSION TRANSACTION READ ONLY")
      {session_failure}
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      if printf '%s\n' "$line" | grep -q sql_mode; then
        {session_response}
      else
        printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
        {finish_status}
      fi
      ;;
    *)
      {user_response}
      ;;
  esac
done
exit "$exit_status"
"#,
    );
    fs::write(&program, script).unwrap();
    let mut permissions = fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&program, permissions).unwrap();
    (directory, program, log_file)
}

fn fake_mysql_single_with_warning() -> (TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let option_file = directory.path().join("option.cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let program = directory.path().join("mysql");
    let log_file = PathBuf::from(format!("{}.log", option_file.display()));
    let script = r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'argv=%s\n' "$*" >> "$log"
eof=$(printf '\004')
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *"$eof"*)
      exit 0
      ;;
    ";"|"SET SESSION autocommit=1, completion_type=NO_CHAIN"|"SET SESSION TRANSACTION READ ONLY"|"SET SESSION TRANSACTION READ WRITE")
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      if printf '%s\n' "$line" | grep -q sql_mode; then
        printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      else
        printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
      fi
      ;;
    *)
      printf '%s\n' '<resultset><row><field name="value">tree</field></row></resultset>'
      printf '%s\n' 'Warning (Code 1265): truncated'
      ;;
  esac
done
"#;
    fs::write(&program, script).unwrap();
    let mut permissions = fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&program, permissions).unwrap();
    (directory, program, log_file)
}

fn fake_mysql_multi() -> (TempDir, PathBuf, PathBuf) {
    fake_mysql_multi_with_mode(false, None, false)
}

fn fake_mysql_multi_with_marker_failure() -> (TempDir, PathBuf, PathBuf) {
    fake_mysql_multi_with_mode(true, None, false)
}

fn fake_mysql_multi_with_statement_failure(error: &str) -> (TempDir, PathBuf, PathBuf) {
    fake_mysql_multi_with_mode(false, Some(error), false)
}

fn fake_mysql_multi_with_tail_failure() -> (TempDir, PathBuf, PathBuf) {
    fake_mysql_multi_with_mode(false, None, true)
}

fn fake_mysql_metadata_columns(fail_read_only: bool) -> (TempDir, PathBuf, PathBuf) {
    fake_mysql_metadata_columns_with_hanging_query(fail_read_only, false)
}

fn fake_mysql_metadata_columns_with_hanging_query(
    fail_read_only: bool,
    hang_after_query: bool,
) -> (TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let option_file = directory.path().join("option.cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let program = directory.path().join("mysql");
    let read_only_failure = if fail_read_only {
        "printf '%s\\n' 'ERROR 1227 (42000): access denied to set transaction read only' >&2\n      exit 1"
    } else {
        ""
    };
    let query_tail = if hang_after_query {
        "while :; do :; done"
    } else {
        ""
    };
    let script = format!(
        r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'process=%s\n' "$$" >> "$log"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *"SET SESSION autocommit=1, completion_type=NO_CHAIN"*)
      ;;
    *"SET SESSION TRANSACTION READ ONLY"*)
      {read_only_failure}
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      printf '%s\t%s\n' '__sabiql_session_marker' '__sabiql_sql_mode'
      printf '%s\t%s\n' "$marker" 'STRICT_TRANS_TABLES'
      ;;
    *"SHOW DATABASES"*)
      printf '%s\n' 'Database'
      {query_tail}
      ;;
  esac
done
exit 0
"#,
    );
    fs::write(&program, script).unwrap();
    let mut permissions = fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&program, permissions).unwrap();
    (directory, program, option_file)
}

fn fake_mysql_multi_with_mode(
    marker_failure: bool,
    statement_error: Option<&str>,
    tail_error: bool,
) -> (TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let option_file = directory.path().join("option.cnf");
    fs::write(&option_file, "[client]\n").unwrap();
    let program = directory.path().join("mysql");
    let update_response =
        statement_error.map_or_else(String::new, |error| format!("printf '%s\\n' '{error}' >&2"));
    let tail = if tail_error {
        "printf '%s\\n' input_closed >> \"$log\"\nprintf '%s\\n' 'ERROR 1054 (42S02): tail error' >&2\n  exit 1"
    } else {
        "exit 0"
    };
    let tail_after_create = if tail_error {
        format!("if [ \"$last_statement\" = create ]; then\n        {tail}\n      fi")
    } else {
        String::new()
    };
    let marker_response = if marker_failure {
        "printf '%s\\n' '<resultset><row><field name=\"wrong\">x</field></row></resultset>'"
            .to_string()
    } else {
        format!("marker=$(printf '%s\\n' \"$line\" | sed \"s/.*SELECT '\\\\([^']*\\\\)' AS __sabiql_marker.*/\\\\1/\")
      case \"$line\" in
        *ROW_COUNT\\(\\)*)
          printf '%s\\n' '<resultset><row><field name=\"__sabiql_marker\">'\"$marker\"'</field><field name=\"affected_rows\">3</field></row></resultset>'
          ;;
        *)
          printf '%s\\n' '<resultset><row><field name=\"__sabiql_marker\">'\"$marker\"'</field></row></resultset>'
          ;;
      esac
      {tail_after_create}")
    };
    let script = format!(
        r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'process=%s\n' "$$" >> "$log"
printf 'argv=%s\n' "$*" >> "$log"
last_statement=none
eof=$(printf '\004')
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *"$eof"*)
      {tail}
      ;;
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_probe.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_lower_case_table_names">0</field></row></resultset>'
      ;;
    "SET SESSION autocommit=1, completion_type=NO_CHAIN"|"SET SESSION TRANSACTION READ ONLY")
      ;;
    ";")
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      if printf '%s\n' "$line" | grep -q sql_mode; then
        printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      else
        printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
      fi
      ;;
    *__sabiql_marker*)
      {marker_response}
      ;;
    *"WITH "*__sabiql_metadata_source*)
      case "$line" in
        *duplicate_alias*)
          printf '%s\n' 'ERROR 1060 (42S21): Duplicate column name duplicate_alias'
          printf '%s\n' '<resultset></resultset>'
          ;;
        *)
          printf '%s\n' '<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row><field name="first_alias" xsi:nil="true"/></row></resultset>'
          ;;
      esac
      ;;
    *__sabiql_metadata_*)
      ;;
    *missing_column*)
      printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2
      exit 1
      ;;
    *SLEEP*)
      while :; do :; done
      ;;
    *"INTO @"*)
      ;;
    *SELECT*)
      case "$line" in
        *WHERE\ FALSE*)
          printf '%s\n' '<resultset></resultset>'
          ;;
        *)
          value=one
          case "$line" in
            *SELECT\ 2*) value=two ;;
            *SELECT\ @picked*) value=picked ;;
          esac
          printf '%s\n' '<resultset><row><field name="value">'"$value"'</field></row></resultset>'
          ;;
      esac
      ;;
    *UPDATE*)
      last_statement=update
      {update_response}
      ;;
    *"SHOW CREATE TABLE"*)
      printf '%s\n' '<resultset><row><field name="Create Table">CREATE TABLE items (id INT)</field></row></resultset>'
      ;;
    *"INSERT IGNORE"*)
      printf '%s\n' 'Warning (Code 1062): duplicate ignored'
      ;;
    *"CREATE TABLE IF NOT EXISTS"*)
      printf '%s\n' 'Note (Code 1050): table already exists'
      last_statement=create
      ;;
    *CREATE*)
      last_statement=create
      ;;
    *)
      printf '%s\n' '<resultset></resultset>'
      ;;
  esac
done
{tail}
"#,
    );
    fs::write(&program, script).unwrap();
    let mut permissions = fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&program, permissions).unwrap();
    (directory, program, option_file)
}

mod adhoc;
mod cleanup;
mod export;
#[path = "metadata.rs"]
mod metadata_session;
mod multi_statement;
mod probe;
#[path = "single.rs"]
mod single_statement;
