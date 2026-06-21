use std::fmt::Write as _;

use crate::app::ports::outbound::{DdlGenerator, SqlDialect};
use crate::domain::{DatabaseType, QueryValue, Table};

use super::SqliteAdapter;

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn text_sql_literal(value: &str) -> String {
    if value.contains('\0') {
        let hex = value
            .as_bytes()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02X}");
                hex
            });
        format!("CAST(X'{hex}' AS TEXT)")
    } else {
        quote_literal(value)
    }
}

fn sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => text_sql_literal(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                let _ = write!(hex, "{byte:02X}");
            }
            format!("X'{hex}'")
        }
    }
}

fn equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = quote_ident(column);
    match value {
        // App-layer write flows reject SQLite NULL primary keys before SQL generation.
        // Reaching this branch means a caller bypassed that guardrail.
        QueryValue::Null => panic!("SQLite write predicates require non-NULL primary key values"),
        _ => format!("{column} = {}", sql_literal(value)),
    }
}

fn row_predicate(pk_pairs: &[(String, QueryValue)]) -> String {
    pk_pairs
        .iter()
        .map(|(col, val)| equality_predicate(col, val))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn rows_predicate(pk_pairs_per_row: &[Vec<(String, QueryValue)>]) -> String {
    let predicates = pk_pairs_per_row
        .iter()
        .map(|pairs| row_predicate(pairs))
        .collect::<Vec<_>>();
    if predicates.len() == 1 {
        predicates[0].clone()
    } else {
        predicates
            .into_iter()
            .map(|predicate| format!("({predicate})"))
            .collect::<Vec<_>>()
            .join(" OR ")
    }
}

pub(super) fn user_tables_query() -> &'static str {
    r"
    WITH fts5_tables AS (
        SELECT name
        FROM sqlite_master
        WHERE type = 'table'
          AND replace(
                  replace(
                      replace(lower(sql), char(13), ' '),
                      char(10), ' '
                  ),
                  char(9), ' '
              ) LIKE 'create%virtual%table%using%fts5%'
    )
    SELECT m.name, m.sql
    FROM sqlite_master m
    WHERE m.type = 'table'
      AND m.name NOT LIKE 'sqlite_%'
      AND NOT EXISTS (
          SELECT 1
          FROM fts5_tables f
          WHERE m.name IN (
              f.name || '_data',
              f.name || '_idx',
              f.name || '_content',
              f.name || '_docsize',
              f.name || '_config'
          )
      )
    ORDER BY name
    "
}

pub(super) fn row_count_query(table: &str) -> String {
    format!("SELECT COUNT(*) AS count FROM {}", quote_ident(table))
}

pub(super) fn encode_preview_column_expr(column: &str) -> String {
    let ident = quote_ident(column);
    format!(
        "CASE WHEN typeof({ident}) = 'text' AND instr({ident}, char(0)) > 0 \
         THEN char(1) || 'SABIQL_HEX:' || hex({ident}) \
         ELSE {ident} END AS {ident}"
    )
}

pub(super) fn build_preview_query(
    table: &str,
    columns: &[String],
    order_columns: &[String],
    limit: usize,
    offset: usize,
) -> String {
    let select_list = if columns.is_empty() {
        "*".to_string()
    } else {
        columns
            .iter()
            .map(|column| encode_preview_column_expr(column))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let order_clause = if order_columns.is_empty() {
        String::new()
    } else {
        let cols = order_columns
            .iter()
            .map(|col| quote_ident(col))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ORDER BY {cols}")
    };

    format!(
        "SELECT {select_list} FROM {}{} LIMIT {} OFFSET {}",
        quote_ident(table),
        order_clause,
        limit,
        offset
    )
}

pub(super) fn table_xinfo_query(table: &str) -> String {
    format!("PRAGMA table_xinfo({})", quote_ident(table))
}

pub(super) fn table_info_query(table: &str) -> String {
    format!("PRAGMA table_info({})", quote_ident(table))
}

pub(super) fn table_definition_query(table: &str) -> String {
    format!(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = {} LIMIT 1",
        quote_literal(table)
    )
}

pub(super) fn index_list_query(table: &str) -> String {
    format!("PRAGMA index_list({})", quote_ident(table))
}

pub(super) fn index_xinfo_query(index: &str) -> String {
    format!("PRAGMA index_xinfo({})", quote_ident(index))
}

pub(super) fn index_definition_query(index: &str) -> String {
    format!(
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = {} LIMIT 1",
        quote_literal(index)
    )
}

pub(super) fn foreign_key_list_query(table: &str) -> String {
    format!("PRAGMA foreign_key_list({})", quote_ident(table))
}

impl DdlGenerator for SqliteAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
        if let Some(source_ddl) = table.source_ddl() {
            return source_ddl.to_string();
        }

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
    fn build_explain_sql(&self, _database_type: DatabaseType, _query: &str) -> Option<String> {
        None
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        _query: &str,
    ) -> Option<String> {
        None
    }

    fn build_update_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        let where_clause = pk_pairs
            .iter()
            .map(|(col, val)| equality_predicate(col, val))
            .collect::<Vec<_>>()
            .join(" AND ");

        format!(
            "UPDATE {}\nSET {} = {}\nWHERE {};",
            quote_ident(table),
            quote_ident(column),
            sql_literal(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let where_clause = rows_predicate(pk_pairs_per_row);

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
            source_ddl: None,
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
    fn sql_literal_preserves_typed_values() {
        assert_eq!(sql_literal(&QueryValue::Null), "NULL");
        assert_eq!(sql_literal(&QueryValue::text("NULL")), "'NULL'");
        assert_eq!(sql_literal(&QueryValue::text("null")), "'null'");
        assert_eq!(sql_literal(&QueryValue::Blob(vec![0, 255])), "X'00FF'");
        assert_eq!(sql_literal(&QueryValue::SqlLiteral("42".to_string())), "42");
        assert_eq!(
            sql_literal(&QueryValue::SqlLiteral("1e999".to_string())),
            "1e999"
        );
    }

    #[test]
    fn user_tables_query_uses_compatible_schema_table() {
        assert!(user_tables_query().contains("FROM sqlite_master"));
        assert!(user_tables_query().contains("name NOT LIKE 'sqlite_%'"));
    }

    #[test]
    fn explain_generation_is_unsupported() {
        let adapter = SqliteAdapter::new();

        assert_eq!(
            adapter.build_explain_sql(DatabaseType::SQLite, "SELECT 1"),
            None
        );
        assert_eq!(
            adapter.build_explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"),
            None
        );
    }

    #[test]
    fn row_count_query_quotes_table_name() {
        assert_eq!(
            row_count_query(r#"my"table"#),
            r#"SELECT COUNT(*) AS count FROM "my""table""#
        );
    }

    #[test]
    fn build_preview_query_orders_by_primary_key_columns_when_available() {
        assert_eq!(
            build_preview_query(
                "users",
                &["id".to_string(), "name".to_string()],
                &["id".to_string()],
                10,
                20
            ),
            concat!(
                r#"SELECT CASE WHEN typeof("id") = 'text' AND instr("id", char(0)) > 0 "#,
                r#"THEN char(1) || 'SABIQL_HEX:' || hex("id") ELSE "id" END AS "id", "#,
                r#"CASE WHEN typeof("name") = 'text' AND instr("name", char(0)) > 0 "#,
                r#"THEN char(1) || 'SABIQL_HEX:' || hex("name") ELSE "name" END AS "name" "#,
                r#"FROM "users" ORDER BY "id" LIMIT 10 OFFSET 20"#
            )
        );
    }

    #[test]
    fn build_preview_query_falls_back_to_star_without_columns() {
        assert_eq!(
            build_preview_query("users", &[], &["id".to_string()], 10, 20),
            r#"SELECT * FROM "users" ORDER BY "id" LIMIT 10 OFFSET 20"#
        );
    }

    #[test]
    fn text_sql_literal_uses_cast_for_embedded_nul_byte() {
        assert_eq!(
            sql_literal(&QueryValue::text("a\0bc")),
            "CAST(X'61006263' AS TEXT)"
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
            index_xinfo_query(r#"my"index"#),
            r#"PRAGMA index_xinfo("my""index")"#
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
                &QueryValue::text("O'Reilly"),
                &[("id".into(), QueryValue::text("42"))],
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
                &QueryValue::text("new"),
                &[
                    ("id".into(), QueryValue::text("1")),
                    ("tenant_id".into(), QueryValue::text("7")),
                ],
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
                &QueryValue::Null,
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = NULL\nWHERE \"id\" = '1';"
            );
        }

        #[test]
        fn text_null_value_generates_quoted_text() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("NULL"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'NULL'\nWHERE \"id\" = '1';"
            );
        }
    }

    mod sql_dialect_bulk_delete {
        use super::*;

        #[test]
        fn single_pk_multiple_rows_returns_or_predicates() {
            let adapter = SqliteAdapter::new();
            let rows = vec![
                vec![("id".to_string(), QueryValue::text("1"))],
                vec![("id".to_string(), QueryValue::text("2"))],
            ];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE (\"id\" = '1') OR (\"id\" = '2');"
            );
        }

        #[test]
        fn composite_pk_returns_or_predicates() {
            let adapter = SqliteAdapter::new();
            let rows = vec![
                vec![
                    ("id".to_string(), QueryValue::text("1")),
                    ("tenant_id".to_string(), QueryValue::text("10")),
                ],
                vec![
                    ("id".to_string(), QueryValue::text("2")),
                    ("tenant_id".to_string(), QueryValue::text("20")),
                ],
            ];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE (\"id\" = '1' AND \"tenant_id\" = '10') OR (\"id\" = '2' AND \"tenant_id\" = '20');"
            );
        }

        #[test]
        #[should_panic(expected = "SQLite write predicates require non-NULL primary key values")]
        fn update_null_pk_value_panics_before_unsafe_predicate() {
            let adapter = SqliteAdapter::new();

            let _ = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("new"),
                &[("id".into(), QueryValue::Null)],
            );
        }

        #[test]
        #[should_panic(expected = "SQLite write predicates require non-NULL primary key values")]
        fn null_pk_value_panics_before_unsafe_predicate() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::Null)]];

            let _ = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);
        }

        #[test]
        #[should_panic(expected = "SQLite write predicates require non-NULL primary key values")]
        fn composite_pk_null_value_panics_before_unsafe_predicate() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![
                ("id".to_string(), QueryValue::Null),
                ("tenant_id".to_string(), QueryValue::text("10")),
            ]];

            let _ = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);
        }

        #[test]
        fn blob_pk_value_uses_blob_literal() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::Blob(vec![0, 255, 65]))]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" = X'00FF41';");
        }

        #[test]
        fn nul_text_pk_value_uses_cast_literal() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![(
                "id".to_string(),
                QueryValue::Text("a\0bc".to_string()),
            )]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE \"id\" = CAST(X'61006263' AS TEXT);"
            );
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
