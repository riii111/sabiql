use std::ffi::OsStr;
use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::RefreshScope;

use super::super::policy::{
    MYSQL_SESSION_MARKER_COLUMN, MySqlExecutionResult, validate_mysql_session_marker,
};
use super::super::xml::parse_mysql_xml;
use super::{
    MYSQL_QUERY_TIMEOUT, MySqlProcess, configure_mysql_session, finish_mysql_session,
    read_one_mysql_resultset_with_diagnostics, run_mysql_process_with_timeout,
    validate_mysql_session_exit, write_mysql_statement,
};

pub(in crate::adapters::mysql) async fn run_mysql_single_statement(
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
) -> Result<MySqlExecutionResult, DbOperationError> {
    let mut process = MySqlProcess::spawn_with_adhoc_program(OsStr::new("mysql"), option_file)?;
    run_mysql_process_with_timeout(
        MYSQL_QUERY_TIMEOUT,
        &mut process,
        RefreshScope::None,
        async |process| {
            run_mysql_single_statement_process_with_diagnostics(process, query, access_mode).await
        },
    )
    .await
}

pub(super) async fn run_mysql_single_statement_process_with_diagnostics(
    process: &mut MySqlProcess,
    query: &str,
    access_mode: AccessMode,
) -> Result<MySqlExecutionResult, DbOperationError> {
    configure_mysql_session(process, access_mode).await?;
    process.probe_sql_mode().await?;

    write_mysql_statement(process, query).await?;
    let (stdout, mut diagnostics) = read_one_mysql_resultset_with_diagnostics(process).await?;
    let result_set = parse_mysql_xml(&stdout)?;

    let session_marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(
        process,
        &format!("SELECT '{session_marker}' AS {MYSQL_SESSION_MARKER_COLUMN}"),
    )
    .await?;
    let (marker_xml, marker_diagnostics) =
        read_one_mysql_resultset_with_diagnostics(process).await?;
    diagnostics.extend(marker_diagnostics);
    let marker_result = parse_mysql_xml(&marker_xml)?;
    validate_mysql_session_marker(&marker_result, &session_marker)?;

    let result = finish_mysql_session(process).await?;
    validate_mysql_session_exit(&result, process.client_packet_limit_bytes)?;

    Ok(MySqlExecutionResult {
        result_set: Some(result_set),
        command_tag: None,
        refresh_scope: RefreshScope::None,
        diagnostics,
    })
}

#[cfg(test)]
pub(super) mod test_support {
    use super::super::super::xml::MySqlResultSet;
    use super::super::read_one_mysql_resultset;
    use super::*;

    pub async fn run_mysql_single_statement_process(
        process: &mut MySqlProcess,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<MySqlResultSet, DbOperationError> {
        configure_mysql_session(process, access_mode).await?;
        process.probe_sql_mode().await?;

        write_mysql_statement(process, query).await?;

        #[cfg(unix)]
        let stdout = read_one_mysql_resultset(process).await?;
        let result = finish_mysql_session(process).await?;
        validate_mysql_session_exit(&result, process.client_packet_limit_bytes)?;
        #[cfg(unix)]
        return parse_mysql_xml(&stdout);
        #[cfg(not(unix))]
        parse_mysql_xml(&result.stdout)
    }
}
