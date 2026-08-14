use super::literal::{encode_preview_column_expr, quote_ident};

pub(in crate::adapters::sqlite) fn build_preview_query(
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
