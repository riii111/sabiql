use crate::infra::utils::{quote_ident, quote_literal};

use super::super::PostgresAdapter;

impl PostgresAdapter {
    pub(in crate::infra::adapters::postgres) fn tables_query() -> &'static str {
        r#"
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
        "#
    }

    pub(in crate::infra::adapters::postgres) fn schemas_query() -> &'static str {
        r#"
        SELECT json_agg(row_to_json(s))
        FROM (
            SELECT nspname as name
            FROM pg_namespace
            WHERE nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND nspname NOT LIKE 'pg_temp_%'
              AND nspname NOT LIKE 'pg_toast_temp_%'
            ORDER BY nspname
        ) s
        "#
    }

    pub(in crate::infra::adapters::postgres) fn columns_query(schema: &str, table: &str) -> String {
        format!(
            r#"
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
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn preview_pk_columns_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r#"
            SELECT COALESCE(json_agg(a.attname ORDER BY array_position(i.indkey, a.attnum)), '[]'::json)
            FROM pg_index i
            JOIN pg_class c ON c.oid = i.indrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
            WHERE i.indisprimary
              AND n.nspname = {}
              AND c.relname = {}
            "#,
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
            format!(" ORDER BY {}", cols)
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
            r#"
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
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn foreign_keys_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r#"
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
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn rls_query(schema: &str, table: &str) -> String {
        format!(
            r#"
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
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn triggers_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r#"
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
            "#,
            quote_literal(schema),
            quote_literal(table)
        )
    }

    pub(in crate::infra::adapters::postgres) fn table_info_query(
        schema: &str,
        table: &str,
    ) -> String {
        format!(
            r#"
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
            "#,
            quote_literal(schema),
            quote_literal(table)
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
        fn hostile_input_is_escaped(#[case] _label: &str, #[case] sql: String) {
            assert!(
                sql.contains(ESCAPED),
                "Hostile input must be quote_literal-escaped in: {sql}"
            );
        }
    }
}
