pub fn escape_preview_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn sql_value_expr(value: &str) -> String {
    if value == "NULL" {
        "NULL".to_string()
    } else {
        quote_literal(value)
    }
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
    use rstest::rstest;

    mod value_expr {
        use super::*;

        #[rstest]
        #[case("NULL", "NULL")]
        #[case("alice", "'alice'")]
        #[case("O'Reilly", "'O''Reilly'")]
        fn value_input_returns_expected_sql_expr(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(sql_value_expr(input), expected);
        }

        #[test]
        fn value_with_control_chars_returns_escaped_preview_value() {
            assert_eq!(escape_preview_value("a\\b\"c\nd"), "a\\\\b\\\"c\\nd");
        }
    }

    mod pk_extraction {
        use super::*;

        #[test]
        fn existing_pk_columns_returns_pk_pairs() {
            let columns = vec!["id".to_string(), "name".to_string()];
            let row = vec!["1".to_string(), "alice".to_string()];
            let pairs = build_pk_pairs(&columns, &row, &["id".to_string()]).unwrap();
            assert_eq!(pairs, vec![("id".to_string(), "1".to_string())]);
        }

        #[test]
        fn missing_pk_column_returns_none() {
            let columns = vec!["name".to_string()];
            let row = vec!["alice".to_string()];
            let pairs = build_pk_pairs(&columns, &row, &["id".to_string()]);
            assert!(pairs.is_none());
        }
    }
}
