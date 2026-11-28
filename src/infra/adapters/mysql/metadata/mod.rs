use crate::app::ports::outbound::DbOperationError;
use crate::domain::connection::MySqlConnectionConfig;
use crate::domain::{DatabaseMetadata, Schema, Table, TableSignatureSnapshot};

use super::adapter::MySqlAdapter;
use super::cli::MySqlResultSet;
use super::dsn::NormalizedMySqlConfig;
use super::sql::{EFFECTIVE_USER_QUERY, EFFECTIVE_USER_RESULT_COLUMNS};

mod catalog;
mod preview;
mod signature;
mod table_detail;

#[cfg(test)]
mod test_support {
    use crate::domain::QueryValue;
    #[cfg(unix)]
    use crate::domain::connection::{MySqlConnectionConfig, MySqlSslMode};

    use super::MySqlResultSet;

    #[cfg(unix)]
    pub(super) fn connection_config(
        user: &str,
        password: &str,
        database: &str,
    ) -> MySqlConnectionConfig {
        MySqlConnectionConfig::new(
            "localhost",
            3306,
            Some(database.to_string()),
            user,
            password,
            MySqlSslMode::Preferred,
        )
    }

    #[cfg(unix)]
    pub(super) fn assert_process_stopped(transcript: &std::path::Path) {
        for _ in 0..200 {
            let pid = std::fs::read_to_string(transcript)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("process=")?.parse::<libc::pid_t>().ok());
            if let Some(pid) = pid
                && unsafe { libc::kill(pid, 0) } == -1
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!(
            "fake mysql process is alive or did not start: {}",
            std::fs::read_to_string(transcript).unwrap()
        );
    }

    pub(super) fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MySqlResultSet {
        MySqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }
}

pub(super) use preview::{convert_preview_values_with_binary_charset, execute_preview};

impl MySqlAdapter {
    pub async fn fetch_metadata(
        &self,
        config: &MySqlConnectionConfig,
    ) -> Result<DatabaseMetadata, DbOperationError> {
        let target = NormalizedMySqlConfig::new(config)?;
        let database = catalog::selected_database(&target)?;
        let tables = catalog::fetch_metadata_snapshot(&target, database).await?;
        let mut metadata = DatabaseMetadata::new(database.to_string());
        metadata.schemas = vec![Schema::new(database.to_string())];
        metadata.table_summaries = tables.into_iter().map(catalog::table_summary).collect();
        Ok(metadata)
    }

    pub async fn fetch_effective_user(
        &self,
        config: &MySqlConnectionConfig,
    ) -> Result<Option<String>, DbOperationError> {
        let target = NormalizedMySqlConfig::new(config)?;
        let (_, result) = catalog::execute_metadata_query(
            &target,
            EFFECTIVE_USER_QUERY,
            EFFECTIVE_USER_RESULT_COLUMNS,
        )
        .await?;
        Ok(effective_user_from_result(&result))
    }

    pub async fn fetch_table_detail(
        &self,
        config: &MySqlConnectionConfig,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        table_detail::fetch_table_detail(config, schema, table).await
    }

    pub async fn fetch_table_columns_and_fks(
        &self,
        config: &MySqlConnectionConfig,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        table_detail::fetch_table_columns_and_fks(config, schema, table).await
    }

    pub async fn fetch_table_columns_and_fks_batch(
        &self,
        config: &MySqlConnectionConfig,
        tables: &[(String, String)],
    ) -> Result<Vec<Table>, DbOperationError> {
        let requested: std::collections::HashSet<_> = tables.iter().collect();
        let details = signature::fetch_table_details(config).await?;
        Ok(details
            .into_iter()
            .filter(|table| requested.contains(&(table.schema.clone(), table.name.clone())))
            .collect())
    }

    pub async fn fetch_table_signatures(
        &self,
        config: &MySqlConnectionConfig,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        signature::fetch_table_signatures(config).await
    }
}

fn effective_user_from_result(result: &MySqlResultSet) -> Option<String> {
    let [row] = result.values.as_slice() else {
        return None;
    };
    let [value] = row.as_slice() else {
        return None;
    };
    let user = value.as_str()?.trim();
    (!user.is_empty()).then(|| user.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::QueryValue;

    fn result(values: Vec<Vec<QueryValue>>) -> MySqlResultSet {
        MySqlResultSet {
            columns: vec!["CURRENT_USER()".to_string()],
            values,
        }
    }

    #[test]
    fn parses_mysql_effective_user_and_trims_server_whitespace() {
        let result = result(vec![vec![QueryValue::text("app_user@%  ")]]);

        assert_eq!(
            effective_user_from_result(&result),
            Some("app_user@%".to_string())
        );
    }

    #[test]
    fn ignores_missing_or_empty_mysql_effective_user_values() {
        assert_eq!(effective_user_from_result(&result(Vec::new())), None);
        assert_eq!(
            effective_user_from_result(&result(vec![vec![QueryValue::text("  ")]])),
            None
        );
    }
}
