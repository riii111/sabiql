fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('\"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn escape_preview_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
}

pub fn sql_value_expr(value: &str) -> String {
    if value == "NULL" {
        "NULL".to_string()
    } else {
        quote_literal(value)
    }
}

pub fn build_update_sql(
    schema: &str,
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
        "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
        quote_ident(schema),
        quote_ident(table),
        quote_ident(column),
        sql_value_expr(new_value),
        where_clause
    )
}

pub fn build_pk_pairs(
    columns: &[String],
    row: &[String],
    pk_columns: &[String],
) -> Option<Vec<(String, String)>> {
    let mut pairs = Vec::with_capacity(pk_columns.len());
    for pk_col in pk_columns {
        let idx = columns.iter().position(|c| c == pk_col)?;
        let value = row.get(idx)?.clone();
        pairs.push((pk_col.clone(), value));
    }
    Some(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_value_is_not_quoted() {
        assert_eq!(sql_value_expr("NULL"), "NULL");
    }

    #[test]
    fn normal_value_is_quoted() {
        assert_eq!(sql_value_expr("alice"), "'alice'");
    }

    #[test]
    fn single_quote_is_escaped() {
        assert_eq!(sql_value_expr("O'Reilly"), "'O''Reilly'");
    }

    #[test]
    fn build_update_sql_escapes_identifier_and_values() {
        let sql = build_update_sql(
            "public",
            "users",
            "name",
            "O'Reilly",
            &[(String::from("id"), String::from("42"))],
        );

        assert_eq!(
            sql,
            "UPDATE \"public\".\"users\"\nSET \"name\" = 'O''Reilly'\nWHERE \"id\" = '42';"
        );
    }

    #[test]
    fn build_update_sql_supports_composite_pk() {
        let sql = build_update_sql(
            "s",
            "t",
            "name",
            "new",
            &[
                (String::from("id"), String::from("1")),
                (String::from("tenant_id"), String::from("7")),
            ],
        );

        assert_eq!(
            sql,
            "UPDATE \"s\".\"t\"\nSET \"name\" = 'new'\nWHERE \"id\" = '1' AND \"tenant_id\" = '7';"
        );
    }

    #[test]
    fn build_pk_pairs_extracts_pk_values() {
        let columns = vec!["id".to_string(), "name".to_string()];
        let row = vec!["1".to_string(), "alice".to_string()];
        let pairs = build_pk_pairs(&columns, &row, &["id".to_string()]).unwrap();
        assert_eq!(pairs, vec![("id".to_string(), "1".to_string())]);
    }

    #[test]
    fn build_pk_pairs_returns_none_when_pk_column_missing() {
        let columns = vec!["name".to_string()];
        let row = vec!["alice".to_string()];
        let pairs = build_pk_pairs(&columns, &row, &["id".to_string()]);
        assert!(pairs.is_none());
    }

    #[test]
    fn value_with_control_chars_returns_escaped_preview_value() {
        assert_eq!(escape_preview_value("a\\b\"c\nd"), "a\\\\b\\\"c\\nd");
    }
}
