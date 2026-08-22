use crate::app::ports::outbound::{
    DbOperationError, MySqlConnectionProbe, MySqlConnectionProbeResult,
};
use async_trait::async_trait;

use super::adapter::MySqlAdapter;
use super::cli::check_mysql_cli_version;
use super::dsn::parse_and_validate_mysql_dsn;
use super::option_file::MySqlOptionFile;

#[async_trait]
impl MySqlConnectionProbe for MySqlAdapter {
    async fn probe(&self, dsn: &str) -> Result<MySqlConnectionProbeResult, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        check_mysql_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let result = super::cli::probe_mysql_server(&option_file.path).await;
        drop(option_file);
        result
    }
}
