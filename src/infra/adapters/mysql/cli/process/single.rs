use std::ffi::OsStr;
use tokio::time::timeout;
use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};

use super::super::error::{classify_mysql_query_failure, has_mysql_cli_error, validate_mode_probe};
use super::super::xml::{MysqlResultSet, parse_mysql_xml};
use super::{
    MYSQL_QUERY_TIMEOUT, MysqlProcess, cleanup_mysql_process, configure_mysql_session,
    finish_mysql_session, finish_mysql_session_after_result, read_one_mysql_resultset,
    write_mysql_statement,
};

pub(in crate::adapters::mysql) async fn run_mysql_single_statement(
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
) -> Result<MysqlResultSet, DbOperationError> {
    let mut process = MysqlProcess::spawn_with_program(OsStr::new("mysql"), option_file)?;
    let result = timeout(
        MYSQL_QUERY_TIMEOUT,
        run_mysql_single_statement_process(&mut process, query, access_mode),
    )
    .await;
    match result {
        Ok(Ok(result_set)) => Ok(result_set),
        Ok(Err(error)) => {
            cleanup_mysql_process(&mut process).await;
            Err(error)
        }
        Err(_) => {
            cleanup_mysql_process(&mut process).await;
            Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            ))
        }
    }
}

pub(super) async fn run_mysql_single_statement_process(
    process: &mut MysqlProcess,
    query: &str,
    access_mode: AccessMode,
) -> Result<MysqlResultSet, DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    let probe_query =
        format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &marker)?;
    configure_mysql_session(process, access_mode).await?;

    write_mysql_statement(process, query).await?;

    #[cfg(unix)]
    let stdout = read_one_mysql_resultset(process).await?;
    let result = if access_mode.is_read_only() {
        finish_mysql_session_after_result(process).await?
    } else {
        finish_mysql_session(process).await?
    };
    if has_mysql_cli_error(&result.error_bytes) {
        return Err(classify_mysql_query_failure(&result.error_bytes));
    }
    if !result.status.success() && !result.forcibly_stopped {
        return Err(classify_mysql_query_failure(&result.error_bytes));
    }
    #[cfg(unix)]
    return parse_mysql_xml(&stdout);
    #[cfg(not(unix))]
    parse_mysql_xml(&result.stdout)
}
