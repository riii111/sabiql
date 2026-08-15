use crate::app::ports::outbound::{AccessMode, ConnectionProbe, DbOperationError};
use async_trait::async_trait;

use super::adapter::MySqlAdapter;
use super::cli::{check_mysql_cli_version, run_mysql_adhoc, validate_mysql_multi_query};
use super::dsn::parse_and_validate_mysql_dsn;
use super::option_file::MySqlOptionFile;

#[async_trait]
impl ConnectionProbe for MySqlAdapter {
    async fn probe(&self, dsn: &str) -> Result<(), DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        self.check_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let result = super::cli::probe_mysql_server(&option_file.path).await;
        drop(option_file);
        result
    }

    async fn fetch_databases(&self, dsn: &str) -> Result<Vec<String>, DbOperationError> {
        let mut target = parse_and_validate_mysql_dsn(dsn)?;
        target.database = None;
        self.check_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let statements = validate_mysql_multi_query("SHOW DATABASES", None, AccessMode::ReadOnly)?;
        let result = run_mysql_adhoc(&option_file.path, &statements, AccessMode::ReadOnly).await;
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
