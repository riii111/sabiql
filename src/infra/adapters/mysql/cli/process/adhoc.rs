use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use uuid::Uuid;

use crate::app::policy::sql::mysql_statement::{MysqlStatement, MysqlStatementKind};
use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::RefreshScope;

use super::super::error::{classify_mysql_query_failure, has_mysql_cli_error, validate_mode_probe};
use super::super::policy::{
    MysqlCommandEvent, MysqlExecutionResult, aggregate_mysql_command_tag,
    is_mysql_row_count_marker, mysql_command_tag,
    mysql_metadata_fallback_has_unsupported_session_state, mysql_refresh_scope,
    mysql_row_count_marker, query_failed_after_change, query_failed_after_mysql_statement,
};
use super::super::xml::MysqlResultSet;
use super::metadata::mysql_metadata_columns;
use super::{
    MYSQL_QUERY_TIMEOUT, MysqlProcess, cleanup_mysql_process, configure_mysql_session,
    finish_mysql_session, read_one_mysql_resultset, write_mysql_statement,
};

pub(in crate::adapters::mysql) async fn run_mysql_adhoc(
    option_file: &Path,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
) -> Result<MysqlExecutionResult, DbOperationError> {
    run_mysql_adhoc_with_program_and_statements_and_expected_columns(
        OsStr::new("mysql"),
        option_file,
        statements,
        access_mode,
        None,
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(in crate::adapters::mysql) async fn run_mysql_adhoc_with_expected_columns(
    option_file: &Path,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
    expected_columns: &[&str],
) -> Result<MysqlExecutionResult, DbOperationError> {
    run_mysql_adhoc_with_program_and_statements_and_expected_columns(
        OsStr::new("mysql"),
        option_file,
        statements,
        access_mode,
        Some(expected_columns),
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(super) async fn run_mysql_adhoc_with_program_and_statements_and_expected_columns(
    program: &OsStr,
    option_file: &Path,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
    expected_columns: Option<&[&str]>,
    execution_timeout: Duration,
) -> Result<MysqlExecutionResult, DbOperationError> {
    if mysql_metadata_fallback_has_unsupported_session_state(statements) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL empty SHOW/DESCRIBE metadata fallback cannot preserve temporary-table session state"
                .to_string(),
        ));
    }
    let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
    let result = tokio::time::timeout(
        execution_timeout,
        run_mysql_adhoc_process(
            &mut process,
            option_file,
            statements,
            access_mode,
            expected_columns,
        ),
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

struct MysqlStatementExecution {
    result_set: Option<MysqlResultSet>,
    command_event: MysqlCommandEvent,
    refresh_scope: RefreshScope,
}

pub(super) async fn fill_mysql_empty_result_columns(
    process: &mut MysqlProcess,
    mut result: MysqlResultSet,
    option_file: &Path,
    query: &str,
    kind: &MysqlStatementKind,
    access_mode: AccessMode,
    expected_columns: Option<&[&str]>,
) -> Result<MysqlResultSet, DbOperationError> {
    if !result.columns.is_empty() || !result.values.is_empty() {
        return Ok(result);
    }
    if let Some(expected_columns) = expected_columns {
        result.columns = expected_columns
            .iter()
            .map(|column| (*column).to_string())
            .collect();
        return Ok(result);
    }
    let fallback_kind =
        super::super::policy::mysql_metadata_fallback_kind(kind).ok_or_else(|| {
            DbOperationError::QueryFailed(
                "MySQL empty result has no supported metadata fallback".to_string(),
            )
        })?;
    result.columns =
        mysql_metadata_columns(process, option_file, query, fallback_kind, access_mode).await?;
    Ok(result)
}

async fn run_mysql_statement(
    process: &mut MysqlProcess,
    option_file: &Path,
    statement: &MysqlStatement,
    access_mode: AccessMode,
    expected_columns: Option<&[&str]>,
    refresh_scope: RefreshScope,
) -> Result<MysqlStatementExecution, DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    let statement_scope = mysql_refresh_scope(&statement.kind);
    let possible_refresh_scope = refresh_scope.merge(statement_scope);
    if let Err(error) = write_mysql_statement(process, &statement.sql).await {
        return Err(query_failed_after_change(error, refresh_scope));
    }
    let marker_query =
        format!("SELECT '{marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows");
    if let Err(error) = write_mysql_statement(process, &marker_query).await {
        return Err(query_failed_after_change(error, possible_refresh_scope));
    }
    let first_xml = match read_one_mysql_resultset(process).await {
        Ok(xml) => xml,
        Err(error) => {
            return Err(query_failed_after_mysql_statement(
                error,
                refresh_scope,
                possible_refresh_scope,
            ));
        }
    };
    let first_result = match super::parse_mysql_xml(&first_xml) {
        Ok(result) => result,
        Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
    };
    let (user_result, marker_result) = if is_mysql_row_count_marker(&first_result, &marker) {
        (None, first_result)
    } else {
        let xml = match read_one_mysql_resultset(process).await {
            Ok(xml) => xml,
            Err(error) => {
                return Err(query_failed_after_mysql_statement(
                    error,
                    refresh_scope,
                    possible_refresh_scope,
                ));
            }
        };
        let marker_result = match super::parse_mysql_xml(&xml) {
            Ok(result) => result,
            Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
        };
        let user_result = fill_mysql_empty_result_columns(
            process,
            first_result,
            option_file,
            &statement.sql,
            &statement.kind,
            access_mode,
            expected_columns,
        )
        .await
        .map_err(|error| query_failed_after_change(error, possible_refresh_scope))?;
        (Some(user_result), marker_result)
    };
    let affected_rows = match mysql_row_count_marker(&marker_result, &marker) {
        Ok(rows) => rows,
        Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
    };
    let tag = mysql_command_tag(&statement.kind, affected_rows, user_result.as_ref());

    Ok(MysqlStatementExecution {
        result_set: user_result,
        command_event: MysqlCommandEvent {
            kind: statement.kind.clone(),
            target: statement.target.clone(),
            tag,
        },
        refresh_scope: possible_refresh_scope,
    })
}

async fn run_mysql_adhoc_process(
    process: &mut MysqlProcess,
    option_file: &Path,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
    expected_columns: Option<&[&str]>,
) -> Result<MysqlExecutionResult, DbOperationError> {
    let probe_marker = Uuid::new_v4().simple().to_string();
    let probe_query = format!(
        "SELECT '{probe_marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode"
    );
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = super::parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &probe_marker)?;
    configure_mysql_session(process, access_mode).await?;

    let mut last_result_set = None;
    let mut command_tags = Vec::with_capacity(statements.len());
    let mut refresh_scope = RefreshScope::None;
    let expected_columns = (statements.len() == 1)
        .then_some(expected_columns)
        .flatten();

    for statement in statements {
        let execution = run_mysql_statement(
            process,
            option_file,
            statement,
            access_mode,
            expected_columns,
            refresh_scope,
        )
        .await?;
        if let Some(result) = execution.result_set {
            last_result_set = Some(result);
        }
        command_tags.push(execution.command_event);
        refresh_scope = execution.refresh_scope;
    }

    let result = finish_mysql_session(process).await?;
    if has_mysql_cli_error(&result.error_bytes) {
        return Err(query_failed_after_change(
            classify_mysql_query_failure(&result.error_bytes),
            refresh_scope,
        ));
    }
    if !result.status.success() && !result.forcibly_stopped {
        return Err(query_failed_after_change(
            classify_mysql_query_failure(&result.error_bytes),
            refresh_scope,
        ));
    }

    Ok(MysqlExecutionResult {
        result_set: last_result_set,
        command_tag: aggregate_mysql_command_tag(&command_tags),
        refresh_scope,
    })
}
