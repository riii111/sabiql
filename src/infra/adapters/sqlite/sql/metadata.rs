use crate::app::ports::outbound::{DbOperationError, SQLITE_TABLE_LIST_REQUIRED_MARKER};

use super::literal::{quote_ident, quote_literal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::adapters::sqlite) enum TableMetadataQueryMode {
    Full,
    FullWithoutRowCount,
    ColumnsAndFks,
}

impl TableMetadataQueryMode {
    const fn include_full_detail(self) -> bool {
        matches!(self, Self::Full | Self::FullWithoutRowCount)
    }

    const fn include_row_count(self) -> bool {
        matches!(self, Self::Full)
    }

    pub(in crate::adapters::sqlite) const fn without_row_count(self) -> Option<Self> {
        match self {
            Self::Full => Some(Self::FullWithoutRowCount),
            Self::FullWithoutRowCount | Self::ColumnsAndFks => None,
        }
    }
}

pub(in crate::adapters::sqlite) fn user_tables_query() -> &'static str {
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
      AND tl.type IN ('table', 'virtual')
      AND tl.name NOT LIKE 'sqlite_%'
    ORDER BY tl.name
    "
}

pub(in crate::adapters::sqlite) fn legacy_user_tables_query() -> &'static str {
    r"
    SELECT name, sql
    FROM sqlite_master
    WHERE type = 'table'
      AND name NOT LIKE 'sqlite_%'
    ORDER BY name
    "
}

pub(in crate::adapters::sqlite) fn has_virtual_tables_query() -> &'static str {
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

pub(in crate::adapters::sqlite) fn table_list_required_error() -> DbOperationError {
    DbOperationError::UnsupportedOperation(format!(
        "{SQLITE_TABLE_LIST_REQUIRED_MARKER}: This database contains virtual tables (such as FTS or RTree). \
         Upgrade sqlite3 to version 3.41.1 or later to browse it safely."
    ))
}

pub(in crate::adapters::sqlite) fn is_table_list_unavailable(error: &str) -> bool {
    error.to_ascii_lowercase().contains("pragma_table_list")
}

pub(in crate::adapters::sqlite) fn preview_metadata_query(table: &str) -> String {
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
                WHERE tl.schema = 'main' AND tl.name = {table} COLLATE NOCASE
                LIMIT 1
            ))
        ) AS payload
        "
    )
}

pub(in crate::adapters::sqlite) fn table_metadata_query(
    table: &str,
    mode: TableMetadataQueryMode,
) -> String {
    let table_literal = quote_literal(table);
    let row_count = if mode.include_row_count() {
        format!("(SELECT COUNT(*) FROM {})", quote_ident(table))
    } else {
        "NULL".to_string()
    };
    let payload = table_metadata_json(&table_literal, &row_count, mode.include_full_detail());
    format!(
        r"
        SELECT {payload} AS payload
        ",
    )
}

pub(in crate::adapters::sqlite) fn table_signatures_query() -> String {
    let payload = table_metadata_json("t.name", "NULL", true);
    format!(
        r"
        SELECT t.name, {payload} AS payload
        FROM (
            SELECT tl.name
            FROM pragma_table_list() AS tl
            WHERE tl.schema = 'main'
              AND tl.type IN ('table', 'virtual')
              AND tl.name NOT LIKE 'sqlite_%'
            ORDER BY tl.name
        ) AS t
        "
    )
}

fn table_metadata_json(table_expr: &str, row_count: &str, include_full_detail: bool) -> String {
    let columns = metadata_columns_json(table_expr);
    let indexes = metadata_indexes_json(table_expr, include_full_detail);
    let foreign_keys = metadata_foreign_keys_json(table_expr);
    let triggers = if include_full_detail {
        format!("json({})", metadata_triggers_json(table_expr))
    } else {
        "json('[]')".to_string()
    };
    let source_ddl = if include_full_detail {
        format!(
            r",
            'source_ddl', (
                SELECT sql FROM sqlite_master
                WHERE type IN ('table', 'view') AND name = {table_expr} COLLATE NOCASE LIMIT 1
            )"
        )
    } else {
        String::new()
    };
    format!(
        r#"json_object(
            'table', json((
                SELECT json_object('type', tl.type, 'wr', tl.wr, 'strict', tl.strict, 'sql', m.sql)
                FROM pragma_table_list() AS tl
                LEFT JOIN sqlite_master AS m
                  ON m.type IN ('table', 'view') AND m.name = tl.name
                WHERE tl.schema = 'main' AND tl.name = {table_expr} COLLATE NOCASE
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
            'row_count', {row_count}
            {source_ddl}
        )"#,
        referenced_columns = metadata_columns_json("r.name"),
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

fn metadata_indexes_json(table_expr: &str, include_full_detail: bool) -> String {
    if !include_full_detail {
        return format!(
            r#"COALESCE((
                SELECT json_group_array(json_object(
                    'name', i.name, 'unique', i."unique", 'partial', i.partial,
                    'columns', json(COALESCE((
                        SELECT json_group_array(json_object(
                            'cid', x.cid, 'name', x.name, 'key', x."key"
                        ))
                        FROM (
                            SELECT * FROM pragma_index_xinfo(i.name)
                            WHERE "key" != 0 ORDER BY seqno
                        ) AS x
                    ), json('[]')))
                ))
                FROM (
                    SELECT * FROM pragma_index_list({table_expr})
                    WHERE "unique" != 0 AND partial = 0 ORDER BY name
                ) AS i
            ), json('[]'))"#
        );
    }
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
                WHERE type = 'trigger' AND tbl_name = {table_expr} COLLATE NOCASE
                ORDER BY name
            ) AS m
        ), json('[]'))"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    mod metadata_queries {
        use super::*;

        #[test]
        fn user_tables_uses_table_list_and_excludes_views() {
            assert!(user_tables_query().contains("pragma_table_list()"));
            assert!(user_tables_query().contains("tl.schema = 'main'"));
            assert!(user_tables_query().contains("tl.type IN ('table', 'virtual')"));
            assert!(user_tables_query().contains("tl.type"));
            assert!(user_tables_query().contains("tl.wr"));
            assert!(user_tables_query().contains("tl.strict"));
            assert!(user_tables_query().contains("name NOT LIKE 'sqlite_%'"));
        }

        #[test]
        fn legacy_user_tables_lists_tables_only() {
            assert!(legacy_user_tables_query().contains("FROM sqlite_master"));
            assert!(legacy_user_tables_query().contains("type = 'table'"));
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

    mod metadata_batch_queries {
        use super::*;

        #[test]
        fn table_detail_combines_metadata_sources() {
            let query = table_metadata_query("users", TableMetadataQueryMode::Full);

            assert!(query.contains("pragma_table_xinfo('users')"));
            assert!(query.contains("pragma_index_list('users')"));
            assert!(query.contains("pragma_foreign_key_list('users')"));
            assert!(query.contains("type = 'trigger'"));
            assert!(query.contains("SELECT COUNT(*) FROM \"users\""));
        }

        #[test]
        fn table_detail_escapes_identifier_and_literal_contexts() {
            let query = table_metadata_query(r#"my'"table"#, TableMetadataQueryMode::Full);

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
            assert!(query.contains("tl.type IN ('table', 'virtual')"));
        }

        #[test]
        fn completion_query_only_loads_unique_index_key_columns() {
            let query = table_metadata_query("users", TableMetadataQueryMode::ColumnsAndFks);

            assert!(query.contains(r#"WHERE "unique" != 0 AND partial = 0"#));
            assert!(query.contains(r#"WHERE "key" != 0"#));
            assert!(!query.contains("'definition'"));
            assert!(!query.contains("'coll'"));
            assert!(!query.contains("'source_ddl'"));
        }

        #[test]
        fn row_count_fallback_keeps_full_detail_payload() {
            let query = table_metadata_query("users", TableMetadataQueryMode::FullWithoutRowCount);

            assert!(!query.contains("SELECT COUNT(*)"));
            assert!(query.contains("'source_ddl'"));
            assert!(query.contains("'definition'"));
        }
    }
}
