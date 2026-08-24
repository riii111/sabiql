use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{
    DatabaseDiagnostic, RefreshScope,
    mysql_sql::{
        MySqlStatement, MySqlStatementKind, has_top_level_user_variable_into_clause,
        mysql_statement_is_data_modifying, mysql_statement_is_schema_modifying,
        mysql_statement_reads_session_diagnostics,
    },
};

use super::super::policy::{
    MySqlExecutionResult, mysql_command_tag, mysql_metadata_fallback_has_unsupported_session_state,
    mysql_possible_refresh_scope, mysql_refresh_scope, mysql_row_count_marker,
    query_failed_after_change,
};
use super::super::xml::MySqlResultSet;
use super::metadata::mysql_metadata_columns_with_diagnostics;
use super::{
    MYSQL_QUERY_TIMEOUT, MySqlProcess, configure_mysql_session, finish_mysql_session,
    read_one_mysql_resultset_with_diagnostics, run_mysql_process_with_timeout,
    validate_mysql_session_exit, write_mysql_statement,
};

pub(in crate::adapters::mysql) async fn run_mysql_adhoc(
    option_file: &Path,
    statements: &[MySqlStatement],
    access_mode: AccessMode,
) -> Result<MySqlExecutionResult, DbOperationError> {
    run_mysql_adhoc_with_program_and_statements(
        OsStr::new("mysql"),
        option_file,
        statements,
        access_mode,
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(super) async fn run_mysql_adhoc_with_program_and_statements(
    program: &OsStr,
    option_file: &Path,
    statements: &[MySqlStatement],
    access_mode: AccessMode,
    execution_timeout: Duration,
) -> Result<MySqlExecutionResult, DbOperationError> {
    if mysql_metadata_fallback_has_unsupported_session_state(statements) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL empty SHOW/DESCRIBE metadata fallback cannot preserve temporary-table session state"
            .to_string(),
        ));
    }
    let possible_refresh_scope = mysql_possible_refresh_scope(statements);
    let mut process = MySqlProcess::spawn_with_adhoc_program(program, option_file)?;
    run_mysql_process_with_timeout(
        execution_timeout,
        &mut process,
        possible_refresh_scope,
        async |process| {
            run_mysql_adhoc_process(process, option_file, statements, access_mode).await
        },
    )
    .await
}

struct MySqlStatementExecution {
    result_set: Option<MySqlResultSet>,
    refresh_scope: RefreshScope,
    diagnostics: Vec<DatabaseDiagnostic>,
}

pub(super) async fn fill_mysql_empty_result_columns(
    process: &mut MySqlProcess,
    mut result: MySqlResultSet,
    option_file: &Path,
    query: &str,
    kind: &MySqlStatementKind,
    access_mode: AccessMode,
    diagnostics: &mut Vec<DatabaseDiagnostic>,
) -> Result<MySqlResultSet, DbOperationError> {
    if !result.columns.is_empty() || !result.values.is_empty() {
        return Ok(result);
    }
    let fallback_kind =
        super::super::policy::mysql_metadata_fallback_kind(kind).ok_or_else(|| {
            DbOperationError::QueryFailed(
                "MySQL empty result has no supported metadata fallback".to_string(),
            )
        })?;
    let (columns, metadata_diagnostics) = mysql_metadata_columns_with_diagnostics(
        process,
        option_file,
        query,
        fallback_kind,
        access_mode,
    )
    .await?;
    diagnostics.extend(metadata_diagnostics);
    result.columns = columns;
    Ok(result)
}

async fn run_mysql_statement(
    process: &mut MySqlProcess,
    statement: &MySqlStatement,
    refresh_scope: RefreshScope,
) -> Result<MySqlStatementExecution, DbOperationError> {
    let statement_scope = mysql_refresh_scope(statement.kind());
    let possible_refresh_scope = refresh_scope.merge(statement_scope);
    if let Err(error) = write_mysql_statement(process, statement.sql()).await {
        return Err(query_failed_after_change(error, refresh_scope));
    }
    if !mysql_statement_returns_resultset(statement) {
        return Ok(MySqlStatementExecution {
            result_set: None,
            refresh_scope: possible_refresh_scope,
            diagnostics: Vec::new(),
        });
    }
    let (xml, diagnostics) = match read_one_mysql_resultset_with_diagnostics(process).await {
        Ok(result) => result,
        Err(error) => {
            return Err(query_failed_after_change(error, possible_refresh_scope));
        }
    };
    let result = match super::parse_mysql_xml(&xml) {
        Ok(result) => result,
        Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
    };

    Ok(MySqlStatementExecution {
        result_set: Some(result),
        refresh_scope: possible_refresh_scope,
        diagnostics,
    })
}

fn mysql_statement_returns_resultset(statement: &MySqlStatement) -> bool {
    if matches!(statement.kind(), MySqlStatementKind::Select)
        && has_top_level_user_variable_into_clause(statement.sql()).unwrap_or(false)
    {
        return false;
    }
    matches!(
        statement.kind(),
        MySqlStatementKind::Select
            | MySqlStatementKind::Table
            | MySqlStatementKind::Show
            | MySqlStatementKind::Describe
    )
}

fn mysql_statement_is_safe_empty_result_metadata_tail(statement: &MySqlStatement) -> bool {
    if mysql_statement_is_schema_modifying(statement.kind()) {
        return !mysql_statement_reads_session_diagnostics(statement.sql()).unwrap_or(true);
    }
    matches!(
        statement.kind(),
        MySqlStatementKind::Begin
            | MySqlStatementKind::StartTransaction
            | MySqlStatementKind::Commit
            | MySqlStatementKind::Rollback
            | MySqlStatementKind::Savepoint
            | MySqlStatementKind::RollbackToSavepoint
            | MySqlStatementKind::ReleaseSavepoint
    )
}

fn mysql_result_needs_metadata(result: &MySqlResultSet) -> bool {
    result.columns.is_empty() && result.values.is_empty()
}

async fn fill_mysql_last_result_columns(
    process: &mut MySqlProcess,
    option_file: &Path,
    last_result_set: &mut Option<MySqlResultSet>,
    last_result_statement: Option<&MySqlStatement>,
    access_mode: AccessMode,
    refresh_scope: RefreshScope,
    diagnostics: &mut Vec<DatabaseDiagnostic>,
) -> Result<(), DbOperationError> {
    let Some(result) = last_result_set.take() else {
        return Ok(());
    };
    let Some(statement) = last_result_statement else {
        *last_result_set = Some(result);
        return Ok(());
    };
    let result = fill_mysql_empty_result_columns(
        process,
        result,
        option_file,
        statement.sql(),
        statement.kind(),
        access_mode,
        diagnostics,
    )
    .await
    .map_err(|error| query_failed_after_change(error, refresh_scope))?;
    *last_result_set = Some(result);
    Ok(())
}

async fn run_mysql_adhoc_process(
    process: &mut MySqlProcess,
    option_file: &Path,
    statements: &[MySqlStatement],
    access_mode: AccessMode,
) -> Result<MySqlExecutionResult, DbOperationError> {
    configure_mysql_session(process, access_mode).await?;
    let mut last_result_set = None;
    let mut last_result_statement = None;
    let mut refresh_scope = RefreshScope::None;
    let mut diagnostics = Vec::new();

    for (index, statement) in statements.iter().enumerate() {
        if last_result_set
            .as_ref()
            .is_some_and(mysql_result_needs_metadata)
            && statements[index..]
                .iter()
                .all(mysql_statement_is_safe_empty_result_metadata_tail)
        {
            fill_mysql_last_result_columns(
                process,
                option_file,
                &mut last_result_set,
                last_result_statement,
                access_mode,
                refresh_scope,
                &mut diagnostics,
            )
            .await?;
        }
        let execution = run_mysql_statement(process, statement, refresh_scope).await?;
        diagnostics.extend(execution.diagnostics);
        if let Some(result) = execution.result_set {
            last_result_set = Some(result);
            last_result_statement = Some(statement);
        }
        refresh_scope = execution.refresh_scope;
    }

    fill_mysql_last_result_columns(
        process,
        option_file,
        &mut last_result_set,
        last_result_statement,
        access_mode,
        refresh_scope,
        &mut diagnostics,
    )
    .await?;

    let marker = Uuid::new_v4().simple().to_string();
    let data_modifying_single =
        statements.len() == 1 && mysql_statement_is_data_modifying(statements[0].kind());
    let marker_query = if data_modifying_single {
        format!("SELECT '{marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows")
    } else {
        format!("SELECT '{marker}' AS __sabiql_marker")
    };
    if let Err(error) = write_mysql_statement(process, &marker_query).await {
        return Err(query_failed_after_change(error, refresh_scope));
    }
    let (marker_xml, marker_diagnostics) =
        match read_one_mysql_resultset_with_diagnostics(process).await {
            Ok(result) => result,
            Err(error) => {
                return Err(query_failed_after_change(error, refresh_scope));
            }
        };
    diagnostics.extend(marker_diagnostics);
    let marker_result = match super::parse_mysql_xml(&marker_xml) {
        Ok(result) => result,
        Err(error) => return Err(query_failed_after_change(error, refresh_scope)),
    };
    let command_tag = if data_modifying_single {
        let affected_rows = mysql_row_count_marker(&marker_result, &marker)
            .map_err(|error| query_failed_after_change(error, refresh_scope))?;
        Some(mysql_command_tag(
            &statements[0],
            affected_rows,
            last_result_set.as_ref(),
        ))
    } else {
        if marker_result.columns != ["__sabiql_marker"]
            || marker_result.values.len() != 1
            || marker_result.values[0].len() != 1
            || marker_result.values[0][0].as_str() != Some(marker.as_str())
        {
            return Err(query_failed_after_change(
                DbOperationError::QueryFailed(
                    "mysql adhoc completion marker did not match".to_string(),
                ),
                refresh_scope,
            ));
        }
        if statements.len() == 1 {
            Some(mysql_command_tag(
                &statements[0],
                0,
                last_result_set.as_ref(),
            ))
        } else {
            None
        }
    };

    let result = finish_mysql_session(process).await?;
    validate_mysql_session_exit(&result, process.client_packet_limit_bytes)
        .map_err(|error| query_failed_after_change(error, refresh_scope))?;

    Ok(MySqlExecutionResult {
        result_set: last_result_set,
        command_tag,
        refresh_scope,
        diagnostics,
    })
}
