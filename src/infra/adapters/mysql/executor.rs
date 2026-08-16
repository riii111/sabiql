use std::time::Instant;

use crate::adapters::csv_export::export_to_downloads;
use crate::app::policy::sql::mysql_statement::mysql_tree_explain_query_kind;
use crate::app::ports::outbound::{AccessMode, DbOperationError, QueryExecutor};
use crate::domain::{QueryResult, QuerySource, WriteExecutionResult};
use async_trait::async_trait;

use super::adapter::MySqlAdapter;
use super::cli::{
    export_mysql_csv_to_file, run_mysql_adhoc, run_mysql_single_statement,
    validate_mysql_export_query, validate_mysql_multi_query,
};
use super::dsn::parse_and_validate_mysql_dsn;
use super::metadata;
use super::option_file::MySqlOptionFile;

#[cfg(feature = "test-support")]
pub(super) mod test_support {
    use std::path::{Path, PathBuf};

    use crate::adapters::csv_export::export_to_path;
    use crate::app::ports::outbound::{AccessMode, DbOperationError};

    use super::{
        MySqlOptionFile, export_mysql_csv_to_file, parse_and_validate_mysql_dsn, run_mysql_adhoc,
        validate_mysql_multi_query,
    };

    #[doc(hidden)]
    pub struct MySqlOptionFileForTest {
        option_file: MySqlOptionFile,
    }

    impl MySqlOptionFileForTest {
        #[must_use]
        pub fn path(&self) -> &Path {
            &self.option_file.path
        }
    }

    #[doc(hidden)]
    pub fn create_mysql_option_file_for_test(
        dsn: &str,
    ) -> Result<MySqlOptionFileForTest, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        Ok(MySqlOptionFileForTest {
            option_file: MySqlOptionFile::create(&target)?,
        })
    }

    #[doc(hidden)]
    /// Runs the normal adhoc CLI process with a read-only session without the app-side policy
    /// gate so integration tests can verify server-side rejection of a side effect.
    pub async fn execute_mysql_adhoc_with_read_only_session_for_test(
        dsn: &str,
        query: &str,
    ) -> Result<(), DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        let statements =
            validate_mysql_multi_query(query, target.database.as_deref(), AccessMode::ReadWrite)?;
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, &statements, AccessMode::ReadOnly).await;
        drop(option_file);
        result.map(|_| ())
    }

    #[cfg(unix)]
    #[doc(hidden)]
    pub async fn run_mysql_cli_script_for_test(
        dsn: &str,
        script: &str,
    ) -> Result<Vec<u8>, DbOperationError> {
        super::super::cli::run_mysql_cli_script_for_test(dsn, script).await
    }

    #[doc(hidden)]
    /// Runs the export process without client-side query policy validation so integration tests can
    /// verify that the MySQL read-only session rejects a side effect at the server boundary.
    pub async fn export_mysql_csv_to_path_for_test(
        dsn: &str,
        query: &str,
        path: PathBuf,
    ) -> Result<PathBuf, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        let query = query.to_string();
        export_to_path(path, move |temporary_path| async move {
            export_mysql_csv_to_file(target, &query, temporary_path).await
        })
        .await
    }
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
            &execution.result_set,
            &preview.visible_columns,
            &preview.identity_columns,
            &preview.binary_charset_columns,
            &preview.binary_charset_identity_columns,
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

        if mysql_tree_explain_query_kind(query).is_some() {
            #[expect(
                clippy::disallowed_methods,
                reason = "infra measures mysql execution time at the I/O boundary"
            )]
            let start = Instant::now();
            let option_file = MySqlOptionFile::create(&target)?;
            let result = run_mysql_single_statement(&option_file.path, query, access_mode).await;
            drop(option_file);
            let result_set = result?;
            return Ok(QueryResult::success_with_values(
                query.to_string(),
                result_set.columns,
                result_set.values,
                start.elapsed().as_millis() as u64,
                QuerySource::Adhoc,
            ));
        }

        let statements =
            validate_mysql_multi_query(query, target.database.as_deref(), access_mode)?;

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, &statements, access_mode).await;
        drop(option_file);
        let execution = result?;
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
            result = result.with_command_tag(tag);
        }
        Ok(result.with_refresh_scope(execution.refresh_scope))
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

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, &statements, access_mode).await;
        drop(option_file);
        let execution = result?;
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
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn count_query_rows(&self, dsn: &str, query: &str) -> Result<usize, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        validate_mysql_export_query(query, target.database.as_deref())?;

        let result = self.execute_adhoc(dsn, query, AccessMode::ReadOnly).await?;
        parse_mysql_count_result(&result)
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

fn parse_mysql_count_result(result: &QueryResult) -> Result<usize, DbOperationError> {
    let value = match result.values() {
        [row] => match row.as_slice() {
            [value] => value.as_str(),
            _ => None,
        },
        _ => None,
    }
    .ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL row count query returned an invalid result".to_string(),
        )
    })?;

    value.parse::<usize>().map_err(|_| {
        DbOperationError::QueryFailed("MySQL row count was not an integer".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::QueryValue;

    fn count_result(values: Vec<Vec<QueryValue>>) -> QueryResult {
        QueryResult::success_with_values(
            "SELECT COUNT(*)".to_string(),
            vec!["COUNT(*)".to_string()],
            values,
            0,
            QuerySource::Adhoc,
        )
    }

    #[test]
    fn parses_a_single_integer_count_result() {
        assert_eq!(
            parse_mysql_count_result(&count_result(vec![vec![QueryValue::text("42")]])).unwrap(),
            42
        );
    }

    #[test]
    fn rejects_an_empty_count_result() {
        assert!(matches!(
            parse_mysql_count_result(&count_result(Vec::new())),
            Err(DbOperationError::QueryFailed(details))
                if details == "MySQL row count query returned an invalid result"
        ));
    }

    #[test]
    fn rejects_a_non_integer_count_result() {
        assert!(matches!(
            parse_mysql_count_result(&count_result(vec![vec![QueryValue::text("unknown")]])),
            Err(DbOperationError::QueryFailed(details))
                if details == "MySQL row count was not an integer"
        ));
    }
}
