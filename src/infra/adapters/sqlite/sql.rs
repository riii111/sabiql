use std::fmt::Write as _;

use crate::app::policy::sql::sqlite_explain::build_sqlite_explain_query_plan_sql;
use crate::app::ports::outbound::{
    DbOperationError, DdlGenerator, SQLITE_TABLE_LIST_REQUIRED_MARKER, SqlDialect,
};
use crate::domain::{DatabaseType, QueryValue, Table, Trigger};

use super::SqliteAdapter;

pub(super) const SQLITE_NUL_TEXT_TRANSPORT_TAG: &str = "SABIQL_HEX:";

pub(super) fn sqlite_nul_text_sentinel() -> String {
    format!("\x01{SQLITE_NUL_TEXT_TRANSPORT_TAG}")
}

pub(super) fn encode_bytes_as_sql_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02X}");
            hex
        })
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn blob_sql_literal(bytes: &[u8]) -> String {
    format!("X'{}'", encode_bytes_as_sql_hex(bytes))
}

fn text_sql_literal(value: &str) -> String {
    if value.contains('\0') {
        format!(
            "CAST(X'{}' AS TEXT)",
            encode_bytes_as_sql_hex(value.as_bytes())
        )
    } else {
        quote_literal(value)
    }
}

fn sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => text_sql_literal(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => blob_sql_literal(bytes),
    }
}

fn equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = quote_ident(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
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
    SELECT tl.name,
           tl.type,
           tl.wr,
           tl.strict,
           m.sql
    FROM pragma_table_list() AS tl
    LEFT JOIN sqlite_master AS m
      ON m.type IN ('table', 'view')
     AND m.name = tl.name
    WHERE tl.schema = 'main'
      AND tl.type IN ('table', 'virtual', 'view')
      AND tl.name NOT LIKE 'sqlite_%'
    ORDER BY tl.name
    "
}

pub(super) fn legacy_user_tables_query() -> &'static str {
    r"
    SELECT name, sql
    FROM sqlite_master
    WHERE type IN ('table', 'view')
      AND name NOT LIKE 'sqlite_%'
    ORDER BY name
    "
}

pub(super) fn has_virtual_tables_query() -> &'static str {
    r"
    SELECT COUNT(*) AS count
    FROM sqlite_master
    WHERE type = 'table'
      AND sql IS NOT NULL
      AND replace(
              replace(
                  replace(lower(sql), char(13), ' '),
                  char(10), ' '
              ),
              char(9), ' '
          ) LIKE 'create%virtual%table%'
    "
}

pub(super) fn table_list_required_error() -> DbOperationError {
    DbOperationError::UnsupportedOperation(format!(
        "{SQLITE_TABLE_LIST_REQUIRED_MARKER}: This database contains virtual tables (such as FTS or RTree). \
         Upgrade sqlite3 to version 3.41.1 or later to browse it safely."
    ))
}

pub(super) fn is_table_list_unavailable(error: &str) -> bool {
    error.to_ascii_lowercase().contains("pragma_table_list")
}

pub(super) const PREVIEW_TRANSPORT_UNISTR_PREFIX: &str = "\\u0001SABIQL_HEX:";
pub(super) fn encode_preview_column_expr(column: &str) -> String {
    let ident = quote_ident(column);
    format!(
        "CASE WHEN typeof({ident}) = 'text' \
         THEN char(1) || '{SQLITE_NUL_TEXT_TRANSPORT_TAG}' || hex({ident}) \
         ELSE {ident} END AS {ident}"
    )
}

pub(super) fn build_preview_query(
    table: &str,
    columns: &[String],
    order_columns: &[String],
    rowid_order_alias: Option<&str>,
    limit: usize,
    offset: usize,
) -> String {
    let visible_select_list = if columns.is_empty() {
        "*".to_string()
    } else {
        columns
            .iter()
            .map(|column| encode_preview_column_expr(column))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let order_clause = if order_columns.is_empty() {
        rowid_order_alias.map_or_else(String::new, |alias| {
            format!(" ORDER BY {}", quote_ident(alias))
        })
    } else {
        let cols = order_columns
            .iter()
            .map(|col| quote_ident(col))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ORDER BY {cols}")
    };

    format!(
        "SELECT {visible_select_list} FROM {}{} LIMIT {} OFFSET {}",
        quote_ident(table),
        order_clause,
        limit,
        offset
    )
}

fn metadata_columns_json(table_expr: &str) -> String {
    format!(
        r#"COALESCE((
            SELECT json_group_array(json_object(
                'cid', c.cid, 'name', c.name, 'type', c.type,
                'notnull', c."notnull", 'dflt_value', c.dflt_value,
                'pk', c.pk, 'hidden', c.hidden
            ))
            FROM (SELECT * FROM pragma_table_xinfo({table_expr}) ORDER BY cid) AS c
        ), json('[]'))"#
    )
}

fn metadata_indexes_json(table_expr: &str) -> String {
    format!(
        r#"COALESCE((
            SELECT json_group_array(json_object(
                'name', i.name, 'unique', i."unique", 'origin', i.origin,
                'partial', i.partial,
                'columns', json(COALESCE((
                    SELECT json_group_array(json_object(
                        'seqno', x.seqno, 'cid', x.cid, 'name', x.name,
                        'desc', x."desc", 'coll', x.coll, 'key', x."key"
                    ))
                    FROM (SELECT * FROM pragma_index_xinfo(i.name) ORDER BY seqno) AS x
                ), json('[]'))),
                'definition', (
                    SELECT m.sql FROM sqlite_master AS m
                    WHERE m.type = 'index' AND m.name = i.name LIMIT 1
                )
            ))
            FROM (SELECT * FROM pragma_index_list({table_expr}) ORDER BY name) AS i
        ), json('[]'))"#
    )
}

fn metadata_foreign_keys_json(table_expr: &str) -> String {
    format!(
        r#"COALESCE((
            SELECT json_group_array(json_object(
                'id', f.id, 'seq', f.seq, 'table', f."table", 'from', f."from",
                'to', f."to", 'on_update', f.on_update, 'on_delete', f.on_delete
            ))
            FROM (
                SELECT * FROM pragma_foreign_key_list({table_expr}) ORDER BY id, seq
            ) AS f
        ), json('[]'))"#
    )
}

fn metadata_triggers_json(table_expr: &str) -> String {
    format!(
        r"COALESCE((
            SELECT json_group_array(json_object('name', m.name, 'sql', m.sql))
            FROM (
                SELECT name, sql FROM sqlite_master
                WHERE type = 'trigger' AND tbl_name = {table_expr}
                ORDER BY name
            ) AS m
        ), json('[]'))"
    )
}

pub(super) fn preview_metadata_query(table: &str) -> String {
    let table = quote_literal(table);
    let columns = metadata_columns_json(&table);
    format!(
        r"
        SELECT json_object(
            'columns', json({columns}),
            'table', json((
                SELECT json_object('type', tl.type, 'wr', tl.wr, 'strict', tl.strict, 'sql', m.sql)
                FROM pragma_table_list() AS tl
                LEFT JOIN sqlite_master AS m
                  ON m.type IN ('table', 'view') AND m.name = tl.name
                WHERE tl.schema = 'main' AND tl.name = {table}
                LIMIT 1
            ))
        ) AS payload
        "
    )
}

fn table_metadata_json(table_expr: &str, row_count: &str, include_triggers: bool) -> String {
    let columns = metadata_columns_json(table_expr);
    let indexes = metadata_indexes_json(table_expr);
    let foreign_keys = metadata_foreign_keys_json(table_expr);
    let triggers = if include_triggers {
        format!("json({})", metadata_triggers_json(table_expr))
    } else {
        "json('[]')".to_string()
    };
    format!(
        r#"json_object(
            'table', json((
                SELECT json_object('type', tl.type, 'wr', tl.wr, 'strict', tl.strict, 'sql', m.sql)
                FROM pragma_table_list() AS tl
                LEFT JOIN sqlite_master AS m
                  ON m.type IN ('table', 'view') AND m.name = tl.name
                WHERE tl.schema = 'main' AND tl.name = {table_expr}
                LIMIT 1
            )),
            'columns', json({columns}),
            'indexes', json({indexes}),
            'foreign_keys', json({foreign_keys}),
            'triggers', {triggers},
            'referenced_columns', json(COALESCE((
                SELECT json_group_array(json_object(
                    'name', r.name,
                    'columns', json({referenced_columns})
                ))
                FROM (
                    SELECT DISTINCT f."table" AS name
                    FROM pragma_foreign_key_list({table_expr}) AS f
                    ORDER BY name
                ) AS r
            ), json('[]'))),
            'row_count', {row_count},
            'source_ddl', (
                SELECT sql FROM sqlite_master
                WHERE type IN ('table', 'view') AND name = {table_expr} LIMIT 1
            )
        )"#,
        referenced_columns = metadata_columns_json("r.name"),
    )
}

pub(super) fn table_metadata_query(table: &str, include_full_detail: bool) -> String {
    let table_literal = quote_literal(table);
    let row_count = if include_full_detail {
        format!("(SELECT COUNT(*) FROM {})", quote_ident(table))
    } else {
        "NULL".to_string()
    };
    let payload = table_metadata_json(&table_literal, &row_count, include_full_detail);
    format!(
        r"
        SELECT {payload} AS payload
        ",
    )
}

pub(super) fn table_signatures_query() -> String {
    let payload = table_metadata_json("t.name", "NULL", true);
    format!(
        r"
        SELECT t.name, {payload} AS payload
        FROM (
            SELECT tl.name
            FROM pragma_table_list() AS tl
            WHERE tl.schema = 'main'
              AND tl.type IN ('table', 'virtual', 'view')
              AND tl.name NOT LIKE 'sqlite_%'
            ORDER BY tl.name
        ) AS t
        "
    )
}

fn terminate_ddl_statement(ddl: &mut String) {
    let trimmed_len = ddl.trim_end().len();
    if trimmed_len == 0 {
        ddl.clear();
        return;
    }
    ddl.truncate(trimmed_len);
    if !ddl.ends_with(';') {
        ddl.push(';');
    }
}

fn append_trigger_ddls(ddl: &mut String, triggers: &[Trigger]) {
    if triggers.is_empty() {
        return;
    }
    terminate_ddl_statement(ddl);
    for trigger in triggers {
        ddl.push('\n');
        ddl.push('\n');
        ddl.push_str(trigger.function_name.trim());
        if !trigger.function_name.trim_end().ends_with(';') {
            ddl.push(';');
        }
    }
}

impl DdlGenerator for SqliteAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
        if let Some(source_ddl) = table.source_ddl() {
            let mut ddl = source_ddl.to_string();
            append_trigger_ddls(&mut ddl, &table.triggers);
            return ddl;
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
        append_trigger_ddls(&mut ddl, &table.triggers);
        ddl
    }
}

impl SqlDialect for SqliteAdapter {
    fn build_explain_sql(&self, _database_type: DatabaseType, query: &str) -> Option<String> {
        build_sqlite_explain_query_plan_sql(query)
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
    use crate::adapters::test_support;

    use super::*;
    use crate::domain::{Column, ColumnAttributes, Trigger, TriggerEvent, TriggerTiming};

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
            columns,
            primary_key,
            ..test_support::minimal_table("", "")
        }
    }

    mod quoting {
        use super::*;

        #[test]
        fn identifier_escapes_embedded_quotes() {
            assert_eq!(quote_ident(r#"my"table"#), r#""my""table""#);
        }

        #[test]
        fn literal_escapes_embedded_quotes() {
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
    }

    mod metadata_queries {
        use super::*;

        #[test]
        fn user_tables_uses_table_list_and_includes_views() {
            assert!(user_tables_query().contains("pragma_table_list()"));
            assert!(user_tables_query().contains("tl.schema = 'main'"));
            assert!(user_tables_query().contains("tl.type IN ('table', 'virtual', 'view')"));
            assert!(user_tables_query().contains("tl.type"));
            assert!(user_tables_query().contains("tl.wr"));
            assert!(user_tables_query().contains("tl.strict"));
            assert!(user_tables_query().contains("name NOT LIKE 'sqlite_%'"));
        }

        #[test]
        fn legacy_user_tables_lists_tables_and_views() {
            assert!(legacy_user_tables_query().contains("FROM sqlite_master"));
            assert!(legacy_user_tables_query().contains("type IN ('table', 'view')"));
            assert!(!legacy_user_tables_query().contains("fts5_tables"));
            assert!(legacy_user_tables_query().contains("name NOT LIKE 'sqlite_%'"));
        }

        #[test]
        fn has_virtual_tables_detects_virtual_table_ddl() {
            assert!(has_virtual_tables_query().contains("create%virtual%table%"));
        }

        #[test]
        fn table_list_required_error_includes_marker_and_upgrade_guidance() {
            let error = table_list_required_error();
            let message = error.user_message();
            assert!(message.contains(SQLITE_TABLE_LIST_REQUIRED_MARKER));
            assert!(message.contains("3.41.1"));
        }

        #[test]
        fn table_list_unavailable_detects_missing_pragma() {
            assert!(is_table_list_unavailable(
                "Error: in prepare, no such table: main.pragma_table_list"
            ));
            assert!(!is_table_list_unavailable("FOREIGN KEY constraint failed"));
        }
    }

    mod explain_queries {
        use super::*;

        #[test]
        fn wraps_select_with_query_plan() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "SELECT 1"),
                Some("EXPLAIN QUERY PLAN SELECT 1".to_string())
            );
            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "WITH cte AS (SELECT 1 AS n) SELECT * FROM cte"
                ),
                Some(
                    "EXPLAIN QUERY PLAN WITH cte AS (SELECT 1 AS n) SELECT * FROM cte".to_string()
                )
            );
        }

        #[test]
        fn wraps_dml_with_query_plan() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "DELETE FROM users"),
                Some("EXPLAIN QUERY PLAN DELETE FROM users".to_string())
            );
            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "UPDATE users SET name = 'a' WHERE id = 1"
                ),
                Some("EXPLAIN QUERY PLAN UPDATE users SET name = 'a' WHERE id = 1".to_string())
            );
            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "INSERT INTO users(name) SELECT name FROM old_users"
                ),
                Some(
                    "EXPLAIN QUERY PLAN INSERT INTO users(name) SELECT name FROM old_users"
                        .to_string()
                )
            );
            assert_eq!(
                adapter
                    .build_explain_sql(DatabaseType::SQLite, "REPLACE INTO users(id) VALUES (1)"),
                Some("EXPLAIN QUERY PLAN REPLACE INTO users(id) VALUES (1)".to_string())
            );
        }

        #[test]
        fn rejects_prefixed_explain_and_analyze() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "EXPLAIN SELECT 1"),
                None
            );
            assert_eq!(
                adapter.build_explain_sql(DatabaseType::SQLite, "CREATE TABLE users(id INTEGER)"),
                None
            );
            assert_eq!(
                adapter.build_explain_analyze_sql(DatabaseType::SQLite, "SELECT 1"),
                None
            );
        }

        #[test]
        fn passes_through_existing_query_plan_prefix() {
            let adapter = SqliteAdapter::new();

            assert_eq!(
                adapter.build_explain_sql(
                    DatabaseType::SQLite,
                    "EXPLAIN QUERY PLAN SELECT * FROM users"
                ),
                Some("EXPLAIN QUERY PLAN SELECT * FROM users".to_string())
            );
        }
    }

    mod preview_queries {
        use super::*;

        #[test]
        fn orders_by_primary_key_columns_when_available() {
            assert_eq!(
                build_preview_query(
                    "users",
                    &["id".to_string(), "name".to_string()],
                    &["id".to_string()],
                    None,
                    10,
                    20
                ),
                concat!(
                    r#"SELECT CASE WHEN typeof("id") = 'text' "#,
                    r#"THEN char(1) || 'SABIQL_HEX:' || hex("id") ELSE "id" END AS "id", "#,
                    r#"CASE WHEN typeof("name") = 'text' "#,
                    r#"THEN char(1) || 'SABIQL_HEX:' || hex("name") ELSE "name" END AS "name" "#,
                    r#"FROM "users" ORDER BY "id" LIMIT 10 OFFSET 20"#
                )
            );
        }

        #[test]
        fn falls_back_to_star_without_columns() {
            assert_eq!(
                build_preview_query("users", &[], &["id".to_string()], None, 10, 20),
                r#"SELECT * FROM "users" ORDER BY "id" LIMIT 10 OFFSET 20"#
            );
        }

        #[test]
        fn primary_keyless_table_orders_by_rowid_without_selecting_it() {
            assert_eq!(
                build_preview_query("logs", &["message".to_string()], &[], Some("rowid"), 10, 0),
                concat!(
                    r#"SELECT CASE WHEN typeof("message") = 'text' "#,
                    r#"THEN char(1) || 'SABIQL_HEX:' || hex("message") ELSE "message" END AS "message" "#,
                    r#"FROM "logs" ORDER BY "rowid" LIMIT 10 OFFSET 0"#
                )
            );
        }
    }

    mod text_literal_encoding {
        use super::*;

        #[test]
        fn uses_cast_for_embedded_nul_byte() {
            assert_eq!(
                sql_literal(&QueryValue::text("a\0bc")),
                "CAST(X'61006263' AS TEXT)"
            );
        }
    }

    mod metadata_batch_queries {
        use super::*;

        #[test]
        fn table_detail_combines_metadata_sources() {
            let query = table_metadata_query("users", true);

            assert!(query.contains("pragma_table_xinfo('users')"));
            assert!(query.contains("pragma_index_list('users')"));
            assert!(query.contains("pragma_foreign_key_list('users')"));
            assert!(query.contains("type = 'trigger'"));
            assert!(query.contains("SELECT COUNT(*) FROM \"users\""));
        }

        #[test]
        fn table_detail_escapes_identifier_and_literal_contexts() {
            let query = table_metadata_query(r#"my'"table"#, true);

            assert!(query.contains(r#"pragma_table_xinfo('my''"table')"#));
            assert!(query.contains(r#"SELECT COUNT(*) FROM "my'""table""#));
        }

        #[test]
        fn signatures_query_batches_all_tables() {
            let query = table_signatures_query();

            assert!(query.contains("SELECT t.name"));
            assert!(query.contains("pragma_table_xinfo(t.name)"));
            assert!(query.contains("pragma_index_list(t.name)"));
            assert!(query.contains("pragma_foreign_key_list(t.name)"));
        }
    }

    mod update_sql {
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

        #[test]
        fn nul_text_value_uses_cast_literal() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("a\0b"),
                &[("id".into(), QueryValue::text("1"))],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = CAST(X'610062' AS TEXT)\nWHERE \"id\" = '1';"
            );
        }
    }

    mod bulk_delete_sql {
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
        fn update_null_predicate_uses_is_null() {
            let adapter = SqliteAdapter::new();

            let sql = adapter.build_update_sql(
                DatabaseType::SQLite,
                "main",
                "users",
                "name",
                &QueryValue::text("new"),
                &[("id".into(), QueryValue::Null)],
            );

            assert_eq!(
                sql,
                "UPDATE \"users\"\nSET \"name\" = 'new'\nWHERE \"id\" IS NULL;"
            );
        }

        #[test]
        fn null_predicate_uses_is_null() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![("id".to_string(), QueryValue::Null)]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(sql, "DELETE FROM \"users\"\nWHERE \"id\" IS NULL;");
        }

        #[test]
        fn composite_null_predicate_uses_is_null() {
            let adapter = SqliteAdapter::new();
            let rows = vec![vec![
                ("id".to_string(), QueryValue::Null),
                ("tenant_id".to_string(), QueryValue::text("10")),
            ]];

            let sql = adapter.build_bulk_delete_sql(DatabaseType::SQLite, "main", "users", &rows);

            assert_eq!(
                sql,
                "DELETE FROM \"users\"\nWHERE \"id\" IS NULL AND \"tenant_id\" = '10';"
            );
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

        #[test]
        fn source_ddl_appends_trigger_definitions() {
            let adapter = SqliteAdapter::new();
            let mut table = make_table(vec![make_column("id", "INTEGER", false)], None);
            table.source_ddl =
                Some("CREATE TABLE \"users\" (\n  \"id\" INTEGER NOT NULL\n)".to_string());
            table.triggers.push(Trigger {
                name: "users_audit".to_string(),
                timing: TriggerTiming::After,
                events: vec![TriggerEvent::Insert],
                function_name:
                    "CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END"
                        .to_string(),
                security_definer: false,
            });

            let ddl = adapter.generate_ddl(DatabaseType::SQLite, &table);

            assert_eq!(
                ddl,
                "CREATE TABLE \"users\" (\n  \"id\" INTEGER NOT NULL\n);\n\nCREATE TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END;"
            );
        }
    }
}
