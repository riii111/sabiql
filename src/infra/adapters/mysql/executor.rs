use std::time::Instant;

use crate::adapters::csv_export::export_to_downloads;
use crate::app::ports::outbound::{AccessMode, DbOperationError, QueryExecutor};
use crate::domain::{
    CommandTag, QueryResult, QuerySource, WriteExecutionResult,
    mysql_sql::mysql_tree_explain_query_kind,
};
use async_trait::async_trait;

use super::adapter::MySqlAdapter;
use super::cli::{
    export_mysql_csv_to_file, probe_mysql_server, run_mysql_adhoc, run_mysql_single_statement,
    validate_mysql_export_query, validate_mysql_multi_query,
    validate_mysql_multi_query_with_lower_case_table_names,
};
use super::dsn::{MySqlDsn, map_mysql_tls_failure, parse_and_validate_mysql_dsn};
use super::metadata;
use super::option_file::MySqlOptionFile;

async fn execute_adhoc_with_target(
    target: &MySqlDsn,
    query: &str,
    access_mode: AccessMode,
) -> Result<QueryResult, DbOperationError> {
    if mysql_tree_explain_query_kind(query).is_some() {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let option_file = MySqlOptionFile::create(target)?;
        let result = run_mysql_single_statement(&option_file.path, query, access_mode).await;
        drop(option_file);
        let execution = result.map_err(|error| map_mysql_tls_failure(error, target.ssl_mode))?;
        let result_set = execution.result_set.ok_or_else(|| {
            DbOperationError::QueryFailed("MySQL TREE EXPLAIN returned no resultset".to_string())
        })?;
        return Ok(QueryResult::success_with_values(
            query.to_string(),
            result_set.columns,
            result_set.values,
            start.elapsed().as_millis() as u64,
            QuerySource::Adhoc,
        )
        .with_mysql_diagnostics(execution.diagnostics));
    }

    let option_file = MySqlOptionFile::create(target)?;
    let lower_case_table_names = probe_mysql_server(&option_file.path)
        .await
        .map_err(|error| map_mysql_tls_failure(error, target.ssl_mode))?
        .lower_case_table_names;
    let statements = validate_mysql_multi_query_with_lower_case_table_names(
        query,
        target.database.as_deref(),
        access_mode,
        lower_case_table_names,
    )?;

    #[expect(
        clippy::disallowed_methods,
        reason = "infra measures mysql execution time at the I/O boundary"
    )]
    let start = Instant::now();
    let result = run_mysql_adhoc(&option_file.path, &statements, access_mode).await;
    drop(option_file);
    let execution = result.map_err(|error| map_mysql_tls_failure(error, target.ssl_mode))?;
    let elapsed = start.elapsed().as_millis() as u64;
    let mut result = match execution.result_set {
        Some(result_set) => QueryResult::success_with_values(
            query.to_string(),
            result_set.columns,
            result_set.values,
            elapsed,
            QuerySource::Adhoc,
        ),
        None => QueryResult::success(
            query.to_string(),
            Vec::new(),
            Vec::new(),
            elapsed,
            QuerySource::Adhoc,
        ),
    };
    if let Some(tag) = execution.command_tag {
        result = with_mysql_command_tag(result, tag)?;
    }
    Ok(result
        .with_mysql_diagnostics(execution.diagnostics)
        .with_refresh_scope(execution.refresh_scope))
}

fn with_mysql_command_tag(
    mut result: QueryResult,
    tag: CommandTag,
) -> Result<QueryResult, DbOperationError> {
    if result.data_row_count() == 0 {
        let affected_rows = match &tag {
            CommandTag::Insert(rows)
            | CommandTag::Affected(rows)
            | CommandTag::Update(rows)
            | CommandTag::Delete(rows) => Some(*rows),
            _ => None,
        };
        if let Some(affected_rows) = affected_rows {
            let row_count = usize::try_from(affected_rows).map_err(|_| {
                DbOperationError::CommandTagParseFailed(
                    "MySQL affected row count does not fit in usize".to_string(),
                )
            })?;
            result = result.with_row_count(row_count);
        }
    }
    Ok(result.with_command_tag(tag))
}

#[async_trait]
impl QueryExecutor for MySqlAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let execution = metadata::execute_preview(dsn, schema, table, limit, offset).await?;
        let preview = execution.metadata;
        let values = metadata::convert_preview_values_with_binary_charset(
            execution.result_set,
            &preview.visible_columns,
            &preview.identity_columns,
        )?;
        let elapsed = start.elapsed().as_millis() as u64;

        let mut query_result = QueryResult::success_with_values(
            execution.display_query,
            preview
                .visible_columns
                .iter()
                .map(|column| column.name.clone())
                .collect(),
            values.visible,
            elapsed,
            QuerySource::Preview,
        );
        if let Some(identity_values) = values.identity {
            query_result = query_result.with_explicit_row_identity(
                preview
                    .identity_columns
                    .iter()
                    .map(|column| column.name.clone())
                    .collect(),
                identity_values,
            );
        }
        Ok(query_result)
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        execute_adhoc_with_target(&target, query, access_mode).await
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        let statements =
            validate_mysql_multi_query(query, target.database.as_deref(), access_mode)?;

        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, &statements, access_mode).await;
        drop(option_file);
        let execution = result.map_err(|error| map_mysql_tls_failure(error, target.ssl_mode))?;
        let affected_rows = execution
            .command_tag
            .and_then(|tag| tag.affected_rows())
            .ok_or_else(|| {
                DbOperationError::CommandTagParseFailed(
                    "MySQL write did not return an affected row count".to_string(),
                )
            })?;
        let affected_rows = usize::try_from(affected_rows).map_err(|_| {
            DbOperationError::CommandTagParseFailed(
                "MySQL affected row count does not fit in usize".to_string(),
            )
        })?;

        Ok(WriteExecutionResult {
            affected_rows,
            diagnostics: execution.diagnostics,
        })
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        validate_mysql_export_query(query, target.database.as_deref())?;

        let query = query.to_string();
        export_to_downloads(file_name, move |path| async move {
            export_mysql_csv_to_file(target, &query, path).await
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::QueryValue;

    fn query_result(values: Vec<Vec<QueryValue>>) -> QueryResult {
        QueryResult::success_with_values(
            "SELECT id FROM users".to_string(),
            vec!["id".to_string()],
            values,
            0,
            QuerySource::Adhoc,
        )
    }

    #[test]
    fn uses_affected_rows_for_a_dml_result_without_payload_rows() {
        let result =
            with_mysql_command_tag(query_result(Vec::new()), CommandTag::Update(3)).unwrap();

        assert_eq!(result.row_count(), 3);
        assert_eq!(result.data_row_count(), 0);
        assert_eq!(result.command_tag, Some(CommandTag::Update(3)));
    }

    #[test]
    fn keeps_payload_row_count_for_a_dml_result() {
        let result = with_mysql_command_tag(
            query_result(vec![vec![QueryValue::text("row")]]),
            CommandTag::Update(3),
        )
        .unwrap();

        assert_eq!(result.row_count(), 1);
        assert_eq!(result.data_row_count(), 1);
    }

    #[test]
    fn keeps_empty_row_count_for_non_dml_command_tags() {
        let result = with_mysql_command_tag(
            query_result(Vec::new()),
            CommandTag::Create("TABLE".to_string()),
        )
        .unwrap();

        assert_eq!(result.row_count(), 0);
    }
}
