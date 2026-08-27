use async_trait::async_trait;

use crate::app::ports::outbound::{DbOperationError, MetadataProvider};
use crate::domain::{
    Column, DatabaseMetadata, Table, TableKindInfo, TableSignatureSnapshot, TableStorageAttributes,
};

use super::PostgresAdapter;

fn extract_primary_key(columns: &[Column]) -> Option<Vec<String>> {
    let pk_cols: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key())
        .map(|c| c.name.clone())
        .collect();
    if pk_cols.is_empty() {
        None
    } else {
        Some(pk_cols)
    }
}

fn postgres_table_not_found(schema: &str, table: &str) -> DbOperationError {
    DbOperationError::ObjectMissing(format!("PostgreSQL table not found: {schema}.{table}"))
}

#[async_trait]
impl MetadataProvider for PostgresAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        let schemas_json = self.execute_query(dsn, Self::schemas_query()).await?;
        let tables_json = self.execute_query(dsn, Self::tables_query()).await?;

        let schemas = Self::parse_schemas(&schemas_json)?;
        let tables = Self::parse_tables(&tables_json)?;

        let db_name = Self::extract_database_name(dsn);
        let mut metadata = DatabaseMetadata::new(db_name);
        metadata.schemas = schemas;
        metadata.table_summaries = tables;

        Ok(metadata)
    }

    async fn fetch_effective_user(&self, dsn: &str) -> Result<Option<String>, DbOperationError> {
        let raw_user = self
            .execute_query(dsn, Self::effective_user_query())
            .await?;
        let user = raw_user.trim();
        Ok((!user.is_empty()).then(|| user.to_string()))
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<TableSignatureSnapshot, DbOperationError> {
        let json = self
            .execute_query(dsn, Self::table_signatures_query())
            .await?;
        Ok(TableSignatureSnapshot {
            signatures: Self::parse_table_signatures(&json)?,
            prefetched_table_details: Vec::new(),
        })
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        let query = Self::table_detail_query(schema, table);
        let json = self.execute_query(dsn, &query).await?;
        let (exists, columns, indexes, foreign_keys, rls, triggers, table_info) =
            Self::parse_table_detail_combined(&json)?;
        if !exists {
            return Err(postgres_table_not_found(schema, table));
        }
        let primary_key = extract_primary_key(&columns);

        Ok(Table {
            schema: schema.to_string(),
            name: table.to_string(),
            owner: table_info.owner,
            columns,
            primary_key,
            foreign_keys,
            indexes,
            rls,
            triggers,
            row_count_estimate: table_info.row_count_estimate,
            comment: table_info.comment,
            source_ddl: None,
            storage_attributes: TableStorageAttributes::default(),
            kind_info: TableKindInfo::default(),
        })
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        let query = Self::table_columns_and_fks_query(schema, table);
        let json = self.execute_query(dsn, &query).await?;
        let (exists, columns, foreign_keys) = Self::parse_table_columns_and_fks(&json)?;
        if !exists {
            return Err(postgres_table_not_found(schema, table));
        }
        let primary_key = extract_primary_key(&columns);

        Ok(Table {
            schema: schema.to_string(),
            name: table.to_string(),
            owner: None,
            columns,
            primary_key,
            foreign_keys,
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
            source_ddl: None,
            storage_attributes: TableStorageAttributes::default(),
            kind_info: TableKindInfo::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ports::outbound::{AccessMode, QueryExecutor};

    const DEFAULT_TEST_DSN: &str = "postgres://dev:dev@localhost:5433/testdb";

    fn postgres_test_dsn() -> String {
        std::env::var("SABIQL_TEST_DSN").unwrap_or_else(|_| DEFAULT_TEST_DSN.to_string())
    }

    fn unique_schema_name() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        format!("sabiql_elt024_{}_{}", std::process::id(), nanos)
    }

    fn assert_object_missing(
        result: Result<Table, DbOperationError>,
        context: &str,
    ) -> Result<(), String> {
        match result {
            Err(DbOperationError::ObjectMissing(_)) => Ok(()),
            other => Err(format!("{context} returned {other:?}")),
        }
    }

    fn assert_permission_denied(
        result: Result<Table, DbOperationError>,
        context: &str,
    ) -> Result<(), String> {
        match result {
            Err(DbOperationError::PermissionDenied(_)) => Ok(()),
            other => Err(format!("{context} returned {other:?}")),
        }
    }

    fn dsn_with_credentials(
        dsn: &str,
        username: &str,
        password: &str,
        database: &str,
    ) -> Result<String, String> {
        let mut url = url::Url::parse(dsn).map_err(|error| error.to_string())?;
        url.set_username(username)
            .map_err(|()| "failed to set PostgreSQL test username".to_string())?;
        url.set_password(Some(password))
            .map_err(|()| "failed to set PostgreSQL test password".to_string())?;
        url.set_path(&format!("/{database}"));
        Ok(url.to_string())
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL"]
    async fn provider_classifies_full_and_light_missing_relations_without_losing_empty_tables() {
        let adapter = PostgresAdapter::new();
        let dsn = postgres_test_dsn();
        let schema = unique_schema_name();
        let qualified = |table: &str| format!(r#""{schema}"."{table}""#);

        let setup = format!(
            r#"
            CREATE SCHEMA "{schema}";
            CREATE TABLE {} (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE {} (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE {} ();
            CREATE TABLE {} (id INTEGER);
            CREATE TABLE {} (id INTEGER);
            "#,
            qualified("existing_full"),
            qualified("existing_light"),
            qualified("zero_columns"),
            qualified("drop_full"),
            qualified("drop_light"),
        );
        let setup_result = adapter
            .execute_adhoc(&dsn, &setup, AccessMode::ReadWrite)
            .await
            .map_err(|error| error.to_string());

        let test_result: Result<(), String> = match setup_result {
            Ok(_) => {
                async {
                    let full = adapter
                        .fetch_table_detail(&dsn, &schema, "existing_full")
                        .await
                        .map_err(|error| format!("full existing fetch failed: {error}"))?;
                    if full.columns.len() != 2 || full.owner.is_none() || full.rls.is_none() {
                        return Err(format!("unexpected full existing table: {full:?}"));
                    }

                    let light = adapter
                        .fetch_table_columns_and_fks(&dsn, &schema, "existing_light")
                        .await
                        .map_err(|error| format!("light existing fetch failed: {error}"))?;
                    if light.columns.len() != 2 {
                        return Err(format!("unexpected light existing table: {light:?}"));
                    }

                    let full_zero = adapter
                        .fetch_table_detail(&dsn, &schema, "zero_columns")
                        .await
                        .map_err(|error| format!("full zero-column fetch failed: {error}"))?;
                    if !full_zero.columns.is_empty()
                        || full_zero.owner.is_none()
                        || full_zero.rls.is_none()
                    {
                        return Err(format!("unexpected full zero-column table: {full_zero:?}"));
                    }

                    let light_zero = adapter
                        .fetch_table_columns_and_fks(&dsn, &schema, "zero_columns")
                        .await
                        .map_err(|error| format!("light zero-column fetch failed: {error}"))?;
                    if !light_zero.columns.is_empty() {
                        return Err(format!(
                            "unexpected light zero-column table: {light_zero:?}"
                        ));
                    }

                    assert_object_missing(
                        adapter
                            .fetch_table_detail(&dsn, &schema, "missing_full")
                            .await,
                        "full missing relation",
                    )?;
                    assert_object_missing(
                        adapter
                            .fetch_table_columns_and_fks(&dsn, &schema, "missing_light")
                            .await,
                        "light missing relation",
                    )?;

                    adapter
                        .execute_adhoc(
                            &dsn,
                            &format!("DROP TABLE {}", qualified("drop_full")),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("drop full fixture failed: {error}"))?;
                    assert_object_missing(
                        adapter.fetch_table_detail(&dsn, &schema, "drop_full").await,
                        "full relation after drop",
                    )?;

                    adapter
                        .execute_adhoc(
                            &dsn,
                            &format!("DROP TABLE {}", qualified("drop_light")),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("drop light fixture failed: {error}"))?;
                assert_object_missing(
                    adapter
                        .fetch_table_columns_and_fks(&dsn, &schema, "drop_light")
                        .await,
                        "light relation after drop",
                    )
                    ?;

                    let permission_role = format!("{schema}_role");
                    let permission_database = format!("{schema}_db");
                    let permission_password = "sabiql_elt024_password";
                    adapter
                        .execute_adhoc(
                            &dsn,
                            &format!(
                                "CREATE ROLE \"{permission_role}\" LOGIN PASSWORD '{permission_password}'"
                            ),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("permission role setup failed: {error}"))?;
                    adapter
                        .execute_adhoc(
                            &dsn,
                            &format!("CREATE DATABASE \"{permission_database}\""),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("permission database setup failed: {error}"))?;
                    adapter
                        .execute_adhoc(
                            &dsn,
                            &format!(
                                "REVOKE CONNECT ON DATABASE \"{permission_database}\" FROM PUBLIC"
                            ),
                            AccessMode::ReadWrite,
                        )
                        .await
                        .map_err(|error| format!("permission ACL setup failed: {error}"))?;

                    let permission_dsn = dsn_with_credentials(
                        &dsn,
                        &permission_role,
                        permission_password,
                        &permission_database,
                    )?;
                    assert_permission_denied(
                        adapter
                            .fetch_table_detail(&permission_dsn, &schema, "existing_full")
                            .await,
                        "full permission failure",
                    )?;
                    assert_permission_denied(
                        adapter
                            .fetch_table_columns_and_fks(
                                &permission_dsn,
                                &schema,
                                "existing_light",
                            )
                            .await,
                        "light permission failure",
                    )
                }
                .await
            }
            Err(error) => Err(error),
        };

        let cleanup_result = adapter
            .execute_adhoc(
                &dsn,
                &format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"),
                AccessMode::ReadWrite,
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string());
        let permission_database = format!("{schema}_db");
        let permission_role = format!("{schema}_role");
        let permission_database_cleanup = adapter
            .execute_adhoc(
                &dsn,
                &format!("DROP DATABASE IF EXISTS \"{permission_database}\""),
                AccessMode::ReadWrite,
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string());
        let permission_role_cleanup = adapter
            .execute_adhoc(
                &dsn,
                &format!("DROP ROLE IF EXISTS \"{permission_role}\""),
                AccessMode::ReadWrite,
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string());

        match (
            test_result,
            cleanup_result,
            permission_database_cleanup,
            permission_role_cleanup,
        ) {
            (Ok(()), Ok(()), Ok(()), Ok(())) => {}
            (Err(test_error), Ok(()), Ok(()), Ok(())) => panic!("{test_error}"),
            (Ok(()), cleanup_schema, cleanup_database, cleanup_role) => {
                panic!(
                    "cleanup failed: schema={cleanup_schema:?}; database={cleanup_database:?}; role={cleanup_role:?}"
                )
            }
            (Err(test_error), cleanup_schema, cleanup_database, cleanup_role) => {
                panic!(
                    "test failed: {test_error}; cleanup failed: schema={cleanup_schema:?}; database={cleanup_database:?}; role={cleanup_role:?}"
                )
            }
        }
    }
}
