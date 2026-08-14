use std::time::Instant;

use crate::adapters::csv_export::export_to_downloads;
use crate::app::policy::sql::mysql_statement::mysql_tree_explain_query_kind;
use crate::app::ports::outbound::{
    AccessMode, ConnectionProbe, DbOperationError, DdlGenerator, DsnBuilder, QueryExecutor,
    SqlDialect,
};
use crate::domain::connection::{ConnectionProfile, DatabaseType};
use crate::domain::{QueryResult, QuerySource, QueryValue, Table, WriteExecutionResult};
use async_trait::async_trait;

use super::cli::{
    check_mysql_cli_version, export_mysql_csv_to_file, run_mysql_adhoc, run_mysql_single_statement,
    validate_mysql_export_query, validate_mysql_multi_query,
};

use super::dsn::{
    build_mysql_dsn, parse_mysql_dsn, validate_mysql_tls_files, validate_mysql_values,
};
use super::option_file::MySqlOptionFile;
use super::{metadata, sql};

pub struct MySqlAdapter;

impl MySqlAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MySqlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "test-support")]
pub(super) mod test_support {
    use std::path::PathBuf;

    use crate::adapters::csv_export::export_to_path;
    use crate::app::ports::outbound::DbOperationError;

    use super::{
        export_mysql_csv_to_file, parse_mysql_dsn, validate_mysql_tls_files, validate_mysql_values,
    };

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
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
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
        let preview = metadata::fetch_preview_metadata(dsn, schema, table).await?;
        let query = metadata::build_preview_query(
            schema,
            table,
            &preview.order_columns,
            &preview.visible_columns,
            &preview.identity_columns,
            limit,
            offset,
        );
        let display_query = metadata::build_preview_query(
            schema,
            table,
            &preview.order_columns,
            &preview.visible_columns,
            &[],
            limit,
            offset,
        );
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        let statements =
            validate_mysql_multi_query(&query, target.database.as_deref(), AccessMode::ReadWrite)?;
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(
            &option_file.path,
            &query,
            &statements,
            AccessMode::ReadWrite,
        )
        .await;
        drop(option_file);
        let result_set = result?.result_set.ok_or_else(|| {
            DbOperationError::MetadataParseFailed(
                "MySQL preview query returned no result set".to_string(),
            )
        })?;
        let values = metadata::convert_preview_values(
            &result_set,
            &preview.visible_columns,
            &preview.identity_columns,
        )?;
        let elapsed = start.elapsed().as_millis() as u64;

        let mut query_result = QueryResult::success_with_values(
            display_query,
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
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;

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
        let result = run_mysql_adhoc(&option_file.path, query, &statements, access_mode).await;
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
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        let statements =
            validate_mysql_multi_query(query, target.database.as_deref(), access_mode)?;

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, query, &statements, access_mode).await;
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
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        validate_mysql_export_query(query, target.database.as_deref())?;

        let result = self.execute_adhoc(dsn, query, AccessMode::ReadOnly).await?;
        let value = result
            .values()
            .first()
            .and_then(|row| row.first())
            .and_then(QueryValue::as_str)
            .ok_or_else(|| {
                DbOperationError::QueryFailed(
                    "MySQL row count query returned an invalid result".to_string(),
                )
            })?;
        value.parse::<usize>().map_err(|_| {
            DbOperationError::QueryFailed("MySQL row count was not an integer".to_string())
        })
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        validate_mysql_export_query(query, target.database.as_deref())?;

        let query = query.to_string();
        export_to_downloads(file_name, move |path| async move {
            export_mysql_csv_to_file(target, &query, path).await
        })
        .await
    }
}

impl DdlGenerator for MySqlAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
        table.source_ddl().unwrap_or_default().to_string()
    }
}

impl SqlDialect for MySqlAdapter {
    fn build_explain_sql(&self, _database_type: DatabaseType, query: &str) -> Option<String> {
        sql::build_explain_sql(query)
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        query: &str,
    ) -> Option<String> {
        sql::build_explain_analyze_sql(query)
    }

    fn build_update_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        sql::build_update_sql(schema, table, column, new_value, pk_pairs)
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        sql::build_bulk_delete_sql(schema, table, pk_pairs_per_row)
    }
}

impl DsnBuilder for MySqlAdapter {
    fn build_dsn(&self, profile: &ConnectionProfile) -> String {
        let config = profile
            .mysql_config()
            .expect("MySQL profile requires MySQL config");
        build_mysql_dsn(config)
    }
}

#[async_trait]
impl ConnectionProbe for MySqlAdapter {
    async fn probe(&self, dsn: &str) -> Result<(), DbOperationError> {
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        self.check_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let result = super::cli::probe_mysql_server(&option_file.path).await;
        drop(option_file);
        result
    }

    async fn fetch_databases(&self, dsn: &str) -> Result<Vec<String>, DbOperationError> {
        let mut target = parse_mysql_dsn(dsn)?;
        target.database = None;
        validate_mysql_values(&target)?;
        self.check_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let statements = validate_mysql_multi_query("SHOW DATABASES", None, AccessMode::ReadWrite)?;
        let result = run_mysql_adhoc(
            &option_file.path,
            "SHOW DATABASES",
            &statements,
            AccessMode::ReadWrite,
        )
        .await;
        drop(option_file);
        result.map(|execution| {
            execution.result_set.map_or_else(Vec::new, |result_set| {
                result_set
                    .values
                    .into_iter()
                    .filter_map(|mut row| row.drain(..).next())
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
        })
    }
}

impl MySqlAdapter {
    async fn check_cli_version(&self) -> Result<(), DbOperationError> {
        check_mysql_cli_version().await
    }
}
