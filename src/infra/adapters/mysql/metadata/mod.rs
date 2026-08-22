use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{DatabaseMetadata, Schema, Table, TableSignatureSnapshot};

use super::adapter::MySqlAdapter;
use super::cli::MySqlResultSet;
use super::dsn::parse_and_validate_mysql_dsn;
use super::sql::{EFFECTIVE_USER_QUERY, EFFECTIVE_USER_RESULT_COLUMNS};

mod catalog;
mod preview;
mod signature;
mod table_detail;

pub(super) use preview::{convert_preview_values_with_binary_charset, execute_preview};

#[async_trait]
impl MetadataProvider for MySqlAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        let database = catalog::selected_database(&target)?;
        let snapshot = catalog::fetch_metadata_snapshot(&target, database).await?;
        let mut metadata = DatabaseMetadata::new(database.to_string());
        metadata.schemas = vec![Schema::new(database.to_string())];
        metadata.table_summaries = snapshot
            .tables
            .into_iter()
            .map(catalog::table_summary)
            .collect();
        Ok(metadata)
    }

    async fn fetch_effective_user(&self, dsn: &str) -> Result<Option<String>, DbOperationError> {
        let target = parse_and_validate_mysql_dsn(dsn)?;
        let (_, result) = catalog::execute_metadata_query(
            &target,
            EFFECTIVE_USER_QUERY,
            EFFECTIVE_USER_RESULT_COLUMNS,
        )
        .await?;
        Ok(effective_user_from_result(&result))
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        table_detail::fetch_table_detail_in_session(dsn, schema, table).await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        table_detail::fetch_table_columns_and_fks(dsn, schema, table).await
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        signature::fetch_table_signatures(dsn).await
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
