use crate::app::ports::outbound::{DbOperationError, MySqlConnectionProbeResult};
use crate::domain::connection::MySqlConnectionConfig;

use super::adapter::MySqlAdapter;
use super::cli::check_mysql_cli_version;
use super::dsn::{NormalizedMySqlConfig, map_mysql_tls_failure};
use super::option_file::MySqlOptionFile;

impl MySqlAdapter {
    pub async fn probe(
        &self,
        config: &MySqlConnectionConfig,
    ) -> Result<MySqlConnectionProbeResult, DbOperationError> {
        let target = NormalizedMySqlConfig::new(config)?;
        check_mysql_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let result = super::cli::probe_mysql_server(&option_file.path).await;
        drop(option_file);
        result
            .map(|probe| MySqlConnectionProbeResult {
                lower_case_table_names: probe.lower_case_table_names,
            })
            .map_err(|error| map_mysql_tls_failure(error, target.ssl_mode))
    }
}
