use async_trait::async_trait;

use crate::app::ports::{DdlGenerator, MetadataError, MetadataProvider, QueryExecutor, SqlDialect};
use crate::domain::{DatabaseMetadata, QueryResult, QuerySource, Table, WriteExecutionResult};

mod dsn;
mod psql;
mod select_guard;
mod sql;

pub struct PostgresAdapter {
    timeout_secs: u64,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataProvider for PostgresAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, MetadataError> {
        let schemas_json = self.execute_query(dsn, Self::schemas_query()).await?;
        let tables_json = self.execute_query(dsn, Self::tables_query()).await?;

        let schemas = Self::parse_schemas(&schemas_json)?;
        let tables = Self::parse_tables(&tables_json)?;

        let db_name = Self::extract_database_name(dsn);
        let mut metadata = DatabaseMetadata::new(db_name);
        metadata.schemas = schemas;
        metadata.tables = tables;

        Ok(metadata)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, MetadataError> {
        let columns_q = Self::columns_query(schema, table);
        let indexes_q = Self::indexes_query(schema, table);
        let fks_q = Self::foreign_keys_query(schema, table);
        let rls_q = Self::rls_query(schema, table);
        let triggers_q = Self::triggers_query(schema, table);
        let table_info_q = Self::table_info_query(schema, table);

        // Execute queries sequentially to avoid connection pool exhaustion
        // on tables with many columns
        // TODO: If performance becomes an issue, consider migrating to controlled parallel
        // execution using semaphores (e.g., tokio::sync::Semaphore) to limit concurrency
        let columns_json = self.execute_query(dsn, &columns_q).await?;
        let indexes_json = self.execute_query(dsn, &indexes_q).await?;
        let fks_json = self.execute_query(dsn, &fks_q).await?;
        let rls_json = self.execute_query(dsn, &rls_q).await?;
        let triggers_json = self.execute_query(dsn, &triggers_q).await?;
        let table_info_json = self.execute_query(dsn, &table_info_q).await?;

        let columns = Self::parse_columns(&columns_json)?;
        let indexes = Self::parse_indexes(&indexes_json)?;
        let foreign_keys = Self::parse_foreign_keys(&fks_json)?;
        let rls = Self::parse_rls(&rls_json)?;
        let triggers = Self::parse_triggers(&triggers_json)?;
        let (owner, comment, row_count_estimate) = Self::parse_table_info(&table_info_json)?;

        let pk_cols: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        let primary_key = if pk_cols.is_empty() {
            None
        } else {
            Some(pk_cols)
        };

        Ok(Table {
            schema: schema.to_string(),
            name: table.to_string(),
            owner,
            columns,
            primary_key,
            foreign_keys,
            indexes,
            rls,
            triggers,
            row_count_estimate,
            comment,
        })
    }
}

#[async_trait]
impl QueryExecutor for PostgresAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, MetadataError> {
        // Editing a cell re-fetches the same page; stable ordering prevents the
        // edited row from shifting position after the refresh.
        let order_columns = self
            .fetch_preview_order_columns(dsn, schema, table)
            .await
            .unwrap_or_default();
        let query = Self::build_preview_query(schema, table, &order_columns, limit, offset);
        self.execute_query_raw(dsn, &query, QuerySource::Preview)
            .await
    }

    async fn execute_adhoc(&self, dsn: &str, query: &str) -> Result<QueryResult, MetadataError> {
        if !select_guard::is_select_query(query) {
            return Err(MetadataError::QueryFailed(
                "Only SELECT queries are supported in SQL modal. Use psql/mycli for DDL/DML operations.".to_string()
            ));
        }

        self.execute_query_raw(dsn, query, QuerySource::Adhoc).await
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
    ) -> Result<WriteExecutionResult, MetadataError> {
        self.execute_write_raw(dsn, query).await
    }
}

fn pg_quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn pg_quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn pg_sql_value_expr(value: &str) -> String {
    if value == "NULL" {
        "NULL".to_string()
    } else {
        pg_quote_literal(value)
    }
}

impl DdlGenerator for PostgresAdapter {
    fn generate_ddl(&self, table: &Table) -> String {
        let mut ddl = format!(
            "CREATE TABLE {}.{} (\n",
            pg_quote_ident(&table.schema),
            pg_quote_ident(&table.name)
        );

        for (i, col) in table.columns.iter().enumerate() {
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map(|d| format!(" DEFAULT {}", d))
                .unwrap_or_default();

            ddl.push_str(&format!(
                "  {} {}{}{}",
                pg_quote_ident(&col.name),
                col.data_type,
                nullable,
                default
            ));

            if i < table.columns.len() - 1 {
                ddl.push(',');
            }
            ddl.push('\n');
        }

        if let Some(pk) = &table.primary_key {
            let quoted_cols: Vec<String> = pk.iter().map(|c| pg_quote_ident(c)).collect();
            ddl.push_str(&format!("  PRIMARY KEY ({})\n", quoted_cols.join(", ")));
        }

        ddl.push_str(");");

        let qualified = format!(
            "{}.{}",
            pg_quote_ident(&table.schema),
            pg_quote_ident(&table.name)
        );

        if let Some(comment) = &table.comment {
            ddl.push_str(&format!(
                "\n\nCOMMENT ON TABLE {} IS {};",
                qualified,
                pg_quote_literal(comment)
            ));
        }

        for col in &table.columns {
            if let Some(comment) = &col.comment {
                ddl.push_str(&format!(
                    "\n\nCOMMENT ON COLUMN {}.{} IS {};",
                    qualified,
                    pg_quote_ident(&col.name),
                    pg_quote_literal(comment)
                ));
            }
        }

        ddl
    }
}

impl SqlDialect for PostgresAdapter {
    fn build_update_sql(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &str,
        pk_pairs: &[(String, String)],
    ) -> String {
        let where_clause = pk_pairs
            .iter()
            .map(|(col, val)| format!("{} = {}", pg_quote_ident(col), pg_quote_literal(val)))
            .collect::<Vec<_>>()
            .join(" AND ");

        format!(
            "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
            pg_quote_ident(schema),
            pg_quote_ident(table),
            pg_quote_ident(column),
            pg_sql_value_expr(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, String)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let pk_count = pk_pairs_per_row[0].len();

        let where_clause = if pk_count == 1 {
            let col = pg_quote_ident(&pk_pairs_per_row[0][0].0);
            let values = pk_pairs_per_row
                .iter()
                .map(|pairs| pg_sql_value_expr(&pairs[0].1))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} IN ({})", col, values)
        } else {
            let cols = pk_pairs_per_row[0]
                .iter()
                .map(|(col, _)| pg_quote_ident(col))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = pk_pairs_per_row
                .iter()
                .map(|pairs| {
                    let vals = pairs
                        .iter()
                        .map(|(_, val)| pg_sql_value_expr(val))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({})", vals)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) IN ({})", cols, rows)
        };

        format!(
            "DELETE FROM {}.{}\nWHERE {};",
            pg_quote_ident(schema),
            pg_quote_ident(table),
            where_clause
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod ddl_generation {
        use super::*;
        use crate::app::ports::DdlGenerator;
        use crate::domain::Column;

        fn make_column(name: &str, data_type: &str, nullable: bool) -> Column {
            Column {
                name: name.to_string(),
                data_type: data_type.to_string(),
                nullable,
                is_primary_key: false,
                default: None,
                is_unique: false,
                comment: None,
                ordinal_position: 0,
            }
        }

        fn make_table(columns: Vec<Column>, primary_key: Option<Vec<String>>) -> Table {
            Table {
                schema: "public".to_string(),
                name: "test_table".to_string(),
                owner: None,
                columns,
                primary_key,
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: None,
                comment: None,
            }
        }

        #[test]
        fn table_with_pk_returns_valid_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(
                vec![
                    make_column("id", "integer", false),
                    make_column("name", "text", true),
                ],
                Some(vec!["id".to_string()]),
            );

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("CREATE TABLE \"public\".\"test_table\""));
            assert!(ddl.contains("\"id\" integer NOT NULL"));
            assert!(ddl.contains("\"name\" text"));
            assert!(ddl.contains("PRIMARY KEY (\"id\")"));
        }

        #[test]
        fn table_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("User accounts".to_string());

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("COMMENT ON TABLE \"public\".\"test_table\" IS 'User accounts';"));
        }

        #[test]
        fn column_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut col = make_column("id", "integer", false);
            col.comment = Some("Primary key".to_string());
            let table = make_table(vec![col], None);

            let ddl = adapter.generate_ddl(&table);

            assert!(
                ddl.contains(
                    "COMMENT ON COLUMN \"public\".\"test_table\".\"id\" IS 'Primary key';"
                )
            );
        }

        #[test]
        fn single_quote_in_comment_is_escaped() {
            let adapter = PostgresAdapter::new();
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("It's a test".to_string());

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("IS 'It''s a test';"));
        }

        #[test]
        fn no_comment_on_when_absent() {
            let adapter = PostgresAdapter::new();
            let table = make_table(vec![make_column("id", "integer", false)], None);

            let ddl = adapter.generate_ddl(&table);

            assert!(!ddl.contains("COMMENT ON"));
        }

        #[test]
        fn default_ddl_line_count_matches_generated_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(vec![make_column("col", "text", true)], None);

            let ddl = adapter.generate_ddl(&table);
            let count = adapter.ddl_line_count(&table);

            assert_eq!(count, ddl.lines().count());
        }
    }

    mod sql_dialect_update {
        use super::*;
        use crate::app::ports::SqlDialect;

        #[test]
        fn single_pk_returns_escaped_sql() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "public",
                "users",
                "name",
                "O'Reilly",
                &[("id".into(), "42".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"public\".\"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
            );
        }

        #[test]
        fn composite_pk_returns_where_with_all_keys() {
            let adapter = PostgresAdapter::new();

            let sql = adapter.build_update_sql(
                "s",
                "t",
                "name",
                "new",
                &[("id".into(), "1".into()), ("tenant_id".into(), "7".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"s\".\"t\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }
    }

    mod sql_dialect_bulk_delete {
        use super::*;
        use crate::app::ports::SqlDialect;

        #[test]
        fn single_pk_single_row_returns_in_clause() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "1".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE \"id\" IN ('1');"
            );
        }

        #[test]
        fn single_pk_multiple_rows_returns_in_clause_with_all_values() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![("id".to_string(), "1".to_string())],
                vec![("id".to_string(), "2".to_string())],
                vec![("id".to_string(), "3".to_string())],
            ];

            let sql = adapter.build_bulk_delete_sql("public", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"users\"\nWHERE \"id\" IN ('1', '2', '3');"
            );
        }

        #[test]
        fn composite_pk_returns_row_constructor_in_clause() {
            let adapter = PostgresAdapter::new();
            let rows = vec![
                vec![
                    ("id".to_string(), "1".to_string()),
                    ("tenant_id".to_string(), "a".to_string()),
                ],
                vec![
                    ("id".to_string(), "2".to_string()),
                    ("tenant_id".to_string(), "b".to_string()),
                ],
            ];

            let sql = adapter.build_bulk_delete_sql("s", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"s\".\"t\"\nWHERE (\"id\", \"tenant_id\") IN (('1', 'a'), ('2', 'b'));"
            );
        }

        #[test]
        fn null_pk_value_uses_null_literal() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "NULL".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(sql, "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IN (NULL);");
        }

        #[test]
        fn pk_value_with_quotes_is_escaped() {
            let adapter = PostgresAdapter::new();
            let rows = vec![vec![("id".to_string(), "O'Reilly".to_string())]];

            let sql = adapter.build_bulk_delete_sql("public", "t", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"public\".\"t\"\nWHERE \"id\" IN ('O''Reilly');"
            );
        }
    }
}
