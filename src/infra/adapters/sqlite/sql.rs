use std::fmt::Write as _;

use crate::app::ports::outbound::{DdlGenerator, SqlDialect};
use crate::domain::{DatabaseType, Table};

use super::SqliteAdapter;

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_literal_or_null(value: &str) -> String {
    if value == "NULL" {
        "NULL".to_string()
    } else {
        quote_literal(value)
    }
}

pub(super) fn user_tables_query() -> &'static str {
    r"
    SELECT name, sql
    FROM sqlite_master
    WHERE type = 'table'
      AND name NOT LIKE 'sqlite_%'
    ORDER BY name
    "
}

pub(super) fn row_count_query(table: &str) -> String {
    format!("SELECT COUNT(*) AS count FROM {}", quote_ident(table))
}

pub(super) fn table_xinfo_query(table: &str) -> String {
    format!("PRAGMA table_xinfo({})", quote_ident(table))
}

pub(super) fn table_info_query(table: &str) -> String {
    format!("PRAGMA table_info({})", quote_ident(table))
}

pub(super) fn index_list_query(table: &str) -> String {
    format!("PRAGMA index_list({})", quote_ident(table))
}

pub(super) fn index_info_query(index: &str) -> String {
    format!("PRAGMA index_info({})", quote_ident(index))
}

pub(super) fn foreign_key_list_query(table: &str) -> String {
    format!("PRAGMA foreign_key_list({})", quote_ident(table))
}

impl DdlGenerator for SqliteAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
        let mut ddl = format!("CREATE TABLE {} (\n", quote_ident(&table.name));
        let has_primary_key = table.primary_key.as_ref().is_some_and(|pk| !pk.is_empty());

        for (i, col) in table.columns.iter().enumerate() {
            let nullable = if col.is_nullable() { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map(|d| format!(" DEFAULT {d}"))
                .unwrap_or_default();

            let _ = write!(
                ddl,
                "  {} {}{}{}",
                quote_ident(&col.name),
                col.data_type,
                nullable,
                default
            );

            if i + 1 < table.columns.len() || has_primary_key {
                ddl.push(',');
            }
            ddl.push('\n');
        }

        if let Some(pk) = &table.primary_key
            && !pk.is_empty()
        {
            let quoted_cols: Vec<String> = pk.iter().map(|c| quote_ident(c)).collect();
            let _ = writeln!(ddl, "  PRIMARY KEY ({})", quoted_cols.join(", "));
        }

        ddl.push_str(");");
        ddl
    }
}

impl SqlDialect for SqliteAdapter {
    fn build_explain_sql(&self, _query: &str) -> Option<String> {
        None
    }

    fn build_explain_analyze_sql(&self, _query: &str) -> Option<String> {
        None
    }

    fn build_update_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
        table: &str,
        column: &str,
        new_value: &str,
        pk_pairs: &[(String, String)],
    ) -> String {
        let where_clause = pk_pairs
            .iter()
            .map(|(col, val)| format!("{} = {}", quote_ident(col), quote_literal(val)))
            .collect::<Vec<_>>()
            .join(" AND ");

        format!(
            "UPDATE {}\nSET {} = {}\nWHERE {};",
            quote_ident(table),
            quote_ident(column),
            sql_literal_or_null(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, String)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let pk_count = pk_pairs_per_row[0].len();
        let where_clause = if pk_count == 1 {
            let col = quote_ident(&pk_pairs_per_row[0][0].0);
            let values = pk_pairs_per_row
                .iter()
                .map(|pairs| quote_literal(&pairs[0].1))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{col} IN ({values})")
        } else {
            let cols = pk_pairs_per_row[0]
                .iter()
                .map(|(col, _)| quote_ident(col))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = pk_pairs_per_row
                .iter()
                .map(|pairs| {
                    let vals = pairs
                        .iter()
                        .map(|(_, val)| quote_literal(val))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({vals})")
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({cols}) IN ({rows})")
        };

        format!(
            "DELETE FROM {}\nWHERE {};",
            quote_ident(table),
            where_clause
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, ColumnAttributes};

    fn make_column(name: &str, data_type: &str, nullable: bool) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            default: None,
            attributes: ColumnAttributes::from_parts(nullable, false, false),
            comment: None,
            ordinal_position: 0,
        }
    }

    fn make_table(columns: Vec<Column>, primary_key: Option<Vec<String>>) -> Table {
        Table {
            schema: "main".to_string(),
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
    fn quote_ident_escapes_embedded_quotes() {
        assert_eq!(quote_ident(r#"my"table"#), r#""my""table""#);
    }

    #[test]
    fn quote_literal_escapes_embedded_quotes() {
        assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
    }

    #[test]
    fn sql_literal_or_null_preserves_only_uppercase_null_as_null() {
        assert_eq!(sql_literal_or_null("NULL"), "NULL");
        assert_eq!(sql_literal_or_null("null"), "'null'");
        assert_eq!(sql_literal_or_null("NULL "), "'NULL '");
    }

    #[test]
    fn user_tables_query_uses_compatible_schema_table() {
        assert!(user_tables_query().contains("FROM sqlite_master"));
        assert!(user_tables_query().contains("name NOT LIKE 'sqlite_%'"));
    }

    #[test]
    fn row_count_query_quotes_table_name() {
        assert_eq!(
            row_count_query(r#"my"table"#),
            r#"SELECT COUNT(*) AS count FROM "my""table""#
        );
    }

    #[test]
    fn pragma_queries_quote_identifiers() {
        assert_eq!(
            table_xinfo_query(r#"my"table"#),
            r#"PRAGMA table_xinfo("my""table")"#
        );
        assert_eq!(
            table_info_query(r#"my"table"#),
            r#"PRAGMA table_info("my""table")"#
        );
        assert_eq!(
            index_list_query(r#"my"table"#),
            r#"PRAGMA index_list("my""table")"#
        );
        assert_eq!(
            foreign_key_list_query(r#"my"table"#),
            r#"PRAGMA foreign_key_list("my""table")"#
        );
        assert_eq!(
            index_info_query(r#"my"index"#),
            r#"PRAGMA index_info("my""index")"#
        );
    }

    mod sql_dialect_update {
        use super::*;

        #[test]
        fn single_pk_omits_schema_and_escapes_sql() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                "O'Reilly",
                &[("id".into(), "42".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
            );
        }

        #[test]
        fn composite_pk_returns_where_with_all_keys() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                "new",
                &[("id".into(), "1".into()), ("tenant_id".into(), "7".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
            );
        }

        #[test]
        fn null_value_generates_unquoted_null() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                "NULL",
                &[("id".into(), "1".into())],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = NULL\nWHERE \"id\" = '1';"
            );
        }
    }

    mod sql_dialect_bulk_delete {
        use super::*;

        #[test]
        fn single_pk_multiple_rows_returns_in_clause() {
            let adapter = SqliteAdapter::new();
            let rows = vec![
                vec![("id".to_string(), "1".to_string())],
                vec![("id".to_string(), "2".to_string())],
            ];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" IN ('1', '2');");
        }

        #[test]
        fn composite_pk_returns_row_value_in_clause() {
            let adapter = SqliteAdapter::new();
            let rows = vec![
                vec![
                    ("id".to_string(), "1".to_string()),
                    ("tenant_id".to_string(), "10".to_string()),
                ],
                vec![
                    ("id".to_string(), "2".to_string()),
                    ("tenant_id".to_string(), "20".to_string()),
                ],
            ];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE (\"id\", \"tenant_id\") IN (('1', '10'), ('2', '20'));"
            );
        }

        #[test]
        fn null_like_string_values_are_quoted_as_literals() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![("id".to_string(), "NULL".to_string())]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" IN ('NULL');");
        }
    }

    mod ddl_generation {
        use super::*;

        #[test]
        fn table_with_pk_returns_schema_free_ddl() {
            let adapter = SqliteAdapter::new();
            let table = make_table(
                vec![
                    make_column("id", "INTEGER", false),
                    make_column("name", "TEXT", true),
                ],
                Some(vec!["id".to_string()]),
            );

            let ddl = adapter.generate_ddl(DatabaseType::SQLite, &table);

            assert!(ddl.contains("CREATE TABLE \"test_table\""));
            assert!(ddl.contains("\"id\" INTEGER NOT NULL"));
            assert!(ddl.contains("\"name\" TEXT"));
            assert!(ddl.contains("PRIMARY KEY (\"id\")"));
            assert!(!ddl.contains("\"main\".\"test_table\""));
        }

        #[test]
        fn composite_primary_key_quotes_all_columns() {
            let adapter = SqliteAdapter::new();
            let table = make_table(
                vec![
                    make_column("tenant_id", "INTEGER", false),
                    make_column("id", "INTEGER", false),
                ],
                Some(vec!["tenant_id".to_string(), "id".to_string()]),
            );

            let ddl = adapter.generate_ddl(DatabaseType::SQLite, &table);

            assert!(ddl.contains("PRIMARY KEY (\"tenant_id\", \"id\")"));
        }

        #[test]
        fn defaults_are_preserved_and_comments_are_omitted() {
            let adapter = SqliteAdapter::new();
            let mut column = make_column("created_at", "TEXT", false);
            column.default = Some("CURRENT_TIMESTAMP".to_string());
            column.comment = Some("created time".to_string());
            let mut table = make_table(vec![column], None);
            table.comment = Some("events".to_string());

            let ddl = adapter.generate_ddl(DatabaseType::SQLite, &table);

            assert!(ddl.contains("\"created_at\" TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP"));
            assert!(!ddl.contains("COMMENT ON"));
        }
    }
}
