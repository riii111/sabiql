use crate::infra::utils::{quote_ident, quote_literal};

use super::super::PostgresAdapter;

impl PostgresAdapter {
    pub(in crate::infra::adapters::postgres) fn tables_query() -> &'static str {
        r"
        SELECT json_agg(row_to_json(t))
        FROM (
            SELECT
                n.nspname as schema,
                c.relname as name,
                c.reltuples::bigint as row_count_estimate,
                c.relrowsecurity as has_rls
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind = 'r'
              AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND (
                  has_table_privilege(c.oid, 'SELECT')
                  OR has_table_privilege(c.oid, 'INSERT')
                  OR has_table_privilege(c.oid, 'UPDATE')
                  OR has_table_privilege(c.oid, 'DELETE')
                  OR has_table_privilege(c.oid, 'TRUNCATE')
                  OR has_table_privilege(c.oid, 'REFERENCES')
                  OR has_table_privilege(c.oid, 'TRIGGER')
              )
            ORDER BY n.nspname, c.relname
        ) t
        "
    }

    pub(in crate::infra::adapters::postgres) fn table_signatures_query() -> &'static str {
        r"
        SELECT json_agg(row_to_json(t))
        FROM (
            SELECT
                n.nspname AS schema,
                c.relname AS name,
                md5(
                    COALESCE((
                        SELECT string_agg(
                            a.attname || ':' || pg_catalog.format_type(a.atttypid, a.atttypmod)
                                || ':' || a.attnotnull::text
                                || ':' || COALESCE(pg_get_expr(d.adbin, d.adrelid), ''),
                            '|' ORDER BY a.attnum
                        )
                        FROM pg_attribute a
                        LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
                        WHERE a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
                    ), '')
                    || '##FK##'
                    || COALESCE((
                        SELECT string_agg(
                            con.conname || ':'
                                || n2.nspname || '.' || c2.relname,
                            '|' ORDER BY con.conname
                        )
                        FROM pg_constraint con
                        JOIN pg_class c2 ON c2.oid = con.confrelid
                        JOIN pg_namespace n2 ON n2.oid = c2.relnamespace
                        WHERE con.conrelid = c.oid AND con.contype = 'f'
                    ), '')
                ) AS signature
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind = 'r'
              AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND (
                  has_table_privilege(c.oid, 'SELECT')
                  OR has_table_privilege(c.oid, 'INSERT')
                  OR has_table_privilege(c.oid, 'UPDATE')
                  OR has_table_privilege(c.oid, 'DELETE')
                  OR has_table_privilege(c.oid, 'TRUNCATE')
                  OR has_table_privilege(c.oid, 'REFERENCES')
                  OR has_table_privilege(c.oid, 'TRIGGER')
              )
            ORDER BY n.nspname, c.relname
        ) t
        "
    }

    pub(in crate::infra::adapters::postgres) fn schemas_query() -> &'static str {
        r"
        SELECT json_agg(row_to_json(s))
        FROM (
            SELECT nspname as name
            FROM pg_namespace
            WHERE nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND nspname NOT LIKE 'pg_temp_%'
              AND nspname NOT LIKE 'pg_toast_temp_%'
            ORDER BY nspname
        ) s
        "
    }

    pub(in crate::infra::adapters::postgres) fn columns_query(schema: &str, table: &str) -> String {
        format!(
            r"
            SELECT json_agg(row_to_json(c) ORDER BY c.ordinal_position)
            FROM (
                SELECT
                    a.attname as name,
                    pg_catalog.format_type(a.atttypid, a.atttypmod) as data_type,
                    NOT a.attnotnull as nullable,
                    pg_get_expr(d.adbin, d.adrelid) as default,
                    EXISTS (
                        SELECT 1 FROM pg_index i
                        WHERE i.indrelid = cl.oid
                          AND i.indisprimary
                          AND a.attnum = ANY(i.indkey)
                    ) as is_primary_key,
                    EXISTS (
                        SELECT 1 FROM pg_index i
                        WHERE i.indrelid = cl.oid
                          AND i.indisunique
                          AND NOT i.indisprimary
                          AND array_length(i.indkey, 1) = 1
                          AND a.attnum = ANY(i.indkey)
                    ) as is_unique,
                    col_description(cl.oid, a.attnum) as comment,
                    a.attnum as ordinal_position
                FROM pg_class cl
                JOIN pg_namespace n ON n.oid = cl.relnamespace
                JOIN pg_attribute a ON a.attrelid = cl.oid
                LEFT JOIN pg_attrdef d ON d.adrelid = cl.oid AND d.adnum = a.attnum
                WHERE n.nspname = {}
                  AND cl.relname = {}
                  AND a.attnum > 0
                  AND NOT a.attisdropped
            ) c
            ",
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn preview_pk_columns_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r"
            SELECT COALESCE(json_agg(a.attname ORDER BY array_position(i.indkey, a.attnum)), '[]'::json)
            FROM pg_index i
            JOIN pg_class c ON c.oid = i.indrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
            WHERE i.indisprimary
              AND n.nspname = {}
              AND c.relname = {}
            ",
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn build_preview_query(
        schema: &str,
        table: &str,
        order_columns: &[String],
        limit: usize,
        offset: usize,
    ) -> String {
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
            "SELECT * FROM {}.{}{} LIMIT {} OFFSET {}",
            quote_ident(schema),
            quote_ident(table),
            order_clause,
            limit,
            offset
        )
    }

    pub(in crate::infra::adapters::postgres) fn indexes_query(schema: &str, table: &str) -> String {
        format!(
            r"
            SELECT json_agg(row_to_json(i))
            FROM (
                SELECT
                    idx.relname as name,
                    array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
                    ix.indisunique as is_unique,
                    ix.indisprimary as is_primary,
                    am.amname as index_type,
                    pg_get_indexdef(idx.oid) as definition
                FROM pg_index ix
                JOIN pg_class idx ON idx.oid = ix.indexrelid
                JOIN pg_class tbl ON tbl.oid = ix.indrelid
                JOIN pg_namespace n ON n.oid = tbl.relnamespace
                JOIN pg_am am ON am.oid = idx.relam
                JOIN pg_attribute a ON a.attrelid = tbl.oid AND a.attnum = ANY(ix.indkey)
                WHERE n.nspname = {}
                  AND tbl.relname = {}
                GROUP BY idx.relname, ix.indisunique, ix.indisprimary, am.amname, idx.oid
                ORDER BY idx.relname
            ) i
            ",
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn foreign_keys_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r"
            SELECT json_agg(row_to_json(fk))
            FROM (
                SELECT
                    con.conname as name,
                    n1.nspname as from_schema,
                    c1.relname as from_table,
                    array_agg(a1.attname ORDER BY array_position(con.conkey, a1.attnum)) as from_columns,
                    n2.nspname as to_schema,
                    c2.relname as to_table,
                    array_agg(a2.attname ORDER BY array_position(con.confkey, a2.attnum)) as to_columns,
                    con.confdeltype as on_delete,
                    con.confupdtype as on_update
                FROM pg_constraint con
                JOIN pg_class c1 ON c1.oid = con.conrelid
                JOIN pg_namespace n1 ON n1.oid = c1.relnamespace
                JOIN pg_class c2 ON c2.oid = con.confrelid
                JOIN pg_namespace n2 ON n2.oid = c2.relnamespace
                JOIN pg_attribute a1 ON a1.attrelid = c1.oid AND a1.attnum = ANY(con.conkey)
                JOIN pg_attribute a2 ON a2.attrelid = c2.oid AND a2.attnum = ANY(con.confkey)
                WHERE con.contype = 'f'
                  AND n1.nspname = {}
                  AND c1.relname = {}
                GROUP BY con.conname, n1.nspname, c1.relname, n2.nspname, c2.relname, con.confdeltype, con.confupdtype
            ) fk
            ",
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn rls_query(schema: &str, table: &str) -> String {
        format!(
            r"
            SELECT json_build_object(
                'enabled', c.relrowsecurity,
                'force', c.relforcerowsecurity,
                'policies', COALESCE((
                    SELECT json_agg(json_build_object(
                        'name', p.polname,
                        'permissive', p.polpermissive,
                        'roles', (
                            SELECT array_agg(r.rolname)
                            FROM pg_roles r
                            WHERE r.oid = ANY(p.polroles)
                        ),
                        'cmd', p.polcmd,
                        'qual', pg_get_expr(p.polqual, p.polrelid),
                        'with_check', pg_get_expr(p.polwithcheck, p.polrelid)
                    ))
                    FROM pg_policy p
                    WHERE p.polrelid = c.oid
                ), '[]'::json)
            )
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = {}
              AND c.relname = {}
            ",
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn triggers_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r"
            SELECT json_agg(row_to_json(t) ORDER BY t.name)
            FROM (
                SELECT
                    tg.tgname AS name,
                    CASE
                        WHEN (tg.tgtype & 2) != 0 THEN 'BEFORE'
                        WHEN (tg.tgtype & 2) = 0 AND (tg.tgtype & 64) != 0 THEN 'INSTEAD OF'
                        ELSE 'AFTER'
                    END AS timing,
                    array_remove(ARRAY[
                        CASE WHEN (tg.tgtype & 4) != 0 THEN 'INSERT' END,
                        CASE WHEN (tg.tgtype & 8) != 0 THEN 'DELETE' END,
                        CASE WHEN (tg.tgtype & 16) != 0 THEN 'UPDATE' END,
                        CASE WHEN (tg.tgtype & 32) != 0 THEN 'TRUNCATE' END
                    ], NULL) AS events,
                    p.proname AS function_name,
                    p.prosecdef AS security_definer
                FROM pg_trigger tg
                JOIN pg_class c ON c.oid = tg.tgrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_proc p ON p.oid = tg.tgfoid
                WHERE NOT tg.tgisinternal
                  AND n.nspname = {}
                  AND c.relname = {}
            ) t
            ",
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn table_info_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r"
            SELECT row_to_json(t)
            FROM (
                SELECT
                    pg_get_userbyid(c.relowner) AS owner,
                    obj_description(c.oid) AS comment,
                    c.reltuples::bigint AS row_count_estimate
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE n.nspname = {}
                  AND c.relname = {}
            ) t
            ",
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn table_columns_and_fks_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r"
            SELECT json_build_object(
                'columns', ({columns}),
                'foreign_keys', ({fks})
            )
            ",
            columns = Self::columns_query(schema, table).trim(),
            fks = Self::foreign_keys_query(schema, table).trim(),
        )
    }

    pub(in crate::infra::adapters::postgres) fn table_detail_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r"
            SELECT json_build_object(
                'columns', ({columns}),
                'indexes', ({indexes}),
                'foreign_keys', ({fks}),
                'rls', ({rls}),
                'triggers', ({triggers}),
                'table_info', ({table_info})
            )
            ",
            columns = Self::columns_query(schema, table).trim(),
            indexes = Self::indexes_query(schema, table).trim(),
            fks = Self::foreign_keys_query(schema, table).trim(),
            rls = Self::rls_query(schema, table).trim(),
            triggers = Self::triggers_query(schema, table).trim(),
            table_info = Self::table_info_query(schema, table).trim(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::adapters::postgres::PostgresAdapter;

    mod preview_query {
        use super::*;

        #[test]
        fn with_primary_key_columns_returns_ordered_preview_query() {
            let sql = PostgresAdapter::build_preview_query(
                "public",
                "users",
                &["id".to_string(), "tenant_id".to_string()],
                100,
                200,
            );

            assert_eq!(
                sql,
                "SELECT * FROM \"public\".\"users\" ORDER BY \"id\", \"tenant_id\" LIMIT 100 OFFSET 200"
            );
        }

        #[test]
        fn without_primary_key_columns_returns_unordered_preview_query() {
            let sql = PostgresAdapter::build_preview_query("public", "users", &[], 100, 0);

            assert_eq!(sql, "SELECT * FROM \"public\".\"users\" LIMIT 100 OFFSET 0");
        }

        #[test]
        fn primary_key_query_returns_json_aggregate_sql() {
            let sql = PostgresAdapter::preview_pk_columns_query("public", "users");

            assert!(
                sql.contains("json_agg(a.attname ORDER BY array_position(i.indkey, a.attnum))")
            );
            assert!(sql.contains("n.nspname = 'public'"));
            assert!(sql.contains("c.relname = 'users'"));
        }
    }

    mod table_signatures_query {
        use super::*;

        #[test]
        fn contains_md5_hash() {
            let sql = PostgresAdapter::table_signatures_query();
            assert!(sql.contains("md5("));
        }

        #[test]
        fn uses_same_filter_as_tables_query() {
            let sig_sql = PostgresAdapter::table_signatures_query();
            let tab_sql = PostgresAdapter::tables_query();

            assert!(sig_sql.contains("c.relkind = 'r'"));
            assert!(
                sig_sql
                    .contains("n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')")
            );
            assert!(sig_sql.contains("has_table_privilege(c.oid, 'SELECT')"));

            // Verify the WHERE clauses share the same filters
            for fragment in [
                "c.relkind = 'r'",
                "n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')",
                "has_table_privilege(c.oid, 'SELECT')",
                "has_table_privilege(c.oid, 'INSERT')",
                "has_table_privilege(c.oid, 'TRIGGER')",
            ] {
                assert!(
                    tab_sql.contains(fragment) && sig_sql.contains(fragment),
                    "Filter mismatch: {fragment}"
                );
            }
        }

        #[test]
        fn includes_fk_separator() {
            let sql = PostgresAdapter::table_signatures_query();
            assert!(sql.contains("'##FK##'"));
        }

        #[test]
        fn returns_json_aggregate() {
            let sql = PostgresAdapter::table_signatures_query();
            assert!(sql.contains("json_agg(row_to_json(t))"));
        }
    }

    mod preview_query_edge_cases {
        use super::*;

        #[test]
        fn schema_name_with_double_quote_is_escaped() {
            let sql = PostgresAdapter::build_preview_query("my\"schema", "users", &[], 100, 0);

            assert_eq!(
                sql,
                "SELECT * FROM \"my\"\"schema\".\"users\" LIMIT 100 OFFSET 0"
            );
        }

        #[test]
        fn table_name_with_double_quote_is_escaped() {
            let sql = PostgresAdapter::build_preview_query("public", "my\"table", &[], 100, 0);

            assert_eq!(
                sql,
                "SELECT * FROM \"public\".\"my\"\"table\" LIMIT 100 OFFSET 0"
            );
        }

        #[test]
        fn order_by_column_with_double_quote_is_escaped() {
            let sql = PostgresAdapter::build_preview_query(
                "public",
                "users",
                &["my\"col".to_string()],
                100,
                0,
            );

            assert_eq!(
                sql,
                "SELECT * FROM \"public\".\"users\" ORDER BY \"my\"\"col\" LIMIT 100 OFFSET 0"
            );
        }
    }

    mod table_detail_query {
        use super::*;

        #[test]
        fn wraps_all_six_categories_in_json_build_object() {
            let sql = PostgresAdapter::table_detail_query("public", "users");

            assert!(sql.contains("json_build_object("));
            for key in [
                "'columns'",
                "'indexes'",
                "'foreign_keys'",
                "'rls'",
                "'triggers'",
                "'table_info'",
            ] {
                assert!(sql.contains(key), "Missing key: {key}");
            }
        }

        #[test]
        fn uses_quoted_schema_and_table() {
            let sql = PostgresAdapter::table_detail_query("my_schema", "my_table");

            assert!(sql.contains("'my_schema'"));
            assert!(sql.contains("'my_table'"));
        }
    }

    mod table_columns_and_fks_query {
        use super::*;

        #[test]
        fn wraps_columns_and_fks_only_in_json_build_object() {
            let sql = PostgresAdapter::table_columns_and_fks_query("public", "users");

            assert!(sql.contains("json_build_object("));
            assert!(sql.contains("'columns'"));
            assert!(sql.contains("'foreign_keys'"));
            assert!(!sql.contains("'indexes'"));
            assert!(!sql.contains("'rls'"));
            assert!(!sql.contains("'triggers'"));
            assert!(!sql.contains("'table_info'"));
        }

        #[test]
        fn uses_quoted_schema_and_table() {
            let sql = PostgresAdapter::table_columns_and_fks_query("my_schema", "my_table");

            assert!(sql.contains("'my_schema'"));
            assert!(sql.contains("'my_table'"));
        }
    }

    mod metadata_query_injection {
        use super::*;
        use rstest::rstest;

        const HOSTILE: &str = "'; DROP TABLE users; --";
        const ESCAPED: &str = "'''; DROP TABLE users; --'";

        #[rstest]
        #[case("columns_query", PostgresAdapter::columns_query(HOSTILE, "t"))]
        #[case(
            "columns_query_table",
            PostgresAdapter::columns_query("public", HOSTILE)
        )]
        #[case("indexes_query", PostgresAdapter::indexes_query(HOSTILE, "t"))]
        #[case(
            "foreign_keys_query",
            PostgresAdapter::foreign_keys_query(HOSTILE, "t")
        )]
        #[case("rls_query", PostgresAdapter::rls_query(HOSTILE, "t"))]
        #[case("triggers_query", PostgresAdapter::triggers_query(HOSTILE, "t"))]
        #[case(
            "table_detail_query",
            PostgresAdapter::table_detail_query(HOSTILE, "t")
        )]
        #[case(
            "table_detail_query_table",
            PostgresAdapter::table_detail_query("public", HOSTILE)
        )]
        #[case(
            "table_columns_and_fks_query",
            PostgresAdapter::table_columns_and_fks_query(HOSTILE, "t")
        )]
        #[case(
            "table_columns_and_fks_query_table",
            PostgresAdapter::table_columns_and_fks_query("public", HOSTILE)
        )]
        fn hostile_input_is_escaped(#[case] _label: &str, #[case] sql: String) {
            assert!(
                sql.contains(ESCAPED),
                "Hostile input must be quote_literal-escaped in: {sql}"
            );
        }
    }
}
