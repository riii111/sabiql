use std::ffi::OsStr;
use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};

use super::super::error::{classify_mysql_query_failure, has_mysql_cli_error, validate_mode_probe};
use super::super::xml::{MySqlResultSet, parse_mysql_xml};
use super::{
    MYSQL_QUERY_TIMEOUT, MySqlProcess, configure_mysql_session, finish_mysql_session,
    read_one_mysql_resultset, run_mysql_process_with_timeout, write_mysql_statement,
};

pub(in crate::adapters::mysql) async fn run_mysql_single_statement(
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
) -> Result<MySqlResultSet, DbOperationError> {
    let mut process = MySqlProcess::spawn_with_program(OsStr::new("mysql"), option_file)?;
    run_mysql_process_with_timeout(MYSQL_QUERY_TIMEOUT, &mut process, async |process| {
        run_mysql_single_statement_process(process, query, access_mode).await
    })
    .await
}

pub(super) async fn run_mysql_single_statement_process(
    process: &mut MySqlProcess,
    query: &str,
    access_mode: AccessMode,
) -> Result<MySqlResultSet, DbOperationError> {
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
    let result = finish_mysql_session(process).await?;
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
