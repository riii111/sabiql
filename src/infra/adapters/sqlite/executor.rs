use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::adapters::csv_export::export_to_downloads;
use crate::app::ports::outbound::{AccessMode, DbOperationError, QueryExecutor};
use crate::domain::{
    CommandTag, QueryResult, QuerySource, RefreshScope, TableKind, TableKindInfo,
    WriteExecutionResult,
};

use super::sqlite3::parser::{
    SqliteStatementPlan, aggregate_sqlite_command_tag, append_changes_query_for_plan,
    command_tag_result, is_sqlite_rerunnable_export_query, last_sqlite_result_set,
    parse_affected_rows, quoted_to_query_result, sqlite_adhoc_execution_query_for_plan,
    sqlite_empty_result_sentinel, sqlite_export_not_rerunnable_error, sqlite_probe_marker,
    sqlite_statement_plan, sqlite_statement_tags, statement_counts_as_select_tag,
    strip_sqlite_probes,
};
use super::{SqliteAdapter, sql};

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
        let (columns, order_columns, kind_info) = self.preview_metadata(path, table).await?;
        let rowid_order_alias =
            Self::preview_rowid_order_alias(&columns, &order_columns, &kind_info);
        let query = sql::build_preview_query(
            table,
            &columns,
            &order_columns,
            rowid_order_alias,
            limit,
            offset,
        );
        let result = self
            .execute_quoted_query(path, &query, QuerySource::Preview, true)
            .await?;
        Ok(result.with_columns_if_empty(columns))
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
            .execute_quote(path, &execution_query, access_mode.is_read_only())
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
        let empty_sentinel = sqlite_empty_result_sentinel(&marker);
        if result
            .columns
            .last()
            .is_some_and(|column| column == &empty_sentinel)
        {
            result = result.without_empty_result_sentinel();
        }
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
        let affected_rows = self
            .execute_changes_query(path, &plan, access_mode.is_read_only())
            .await?;
        Ok(WriteExecutionResult {
            affected_rows,
            diagnostics: Vec::new(),
        })
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

impl SqliteAdapter {
    fn preview_rowid_order_alias(
        visible_columns: &[String],
        order_columns: &[String],
        kind_info: &TableKindInfo,
    ) -> Option<&'static str> {
        if !order_columns.is_empty() {
            return None;
        }
        if kind_info.kind != TableKind::Table || kind_info.without_rowid {
            return None;
        }
        ["rowid", "_rowid_", "oid"].into_iter().find(|alias| {
            !visible_columns
                .iter()
                .any(|column| column.eq_ignore_ascii_case(alias))
        })
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
    ) -> Result<usize, DbOperationError> {
        let stdout = self
            .cli
            .execute_csv(path, &append_changes_query_for_plan(plan), read_only)
            .await?;
        parse_affected_rows(&stdout).map_err(|error| DbOperationError::QueryFailedAfterChange {
            source: Arc::new(error),
            refresh_scope: RefreshScope::Data,
        })
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
