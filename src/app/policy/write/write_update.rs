use crate::domain::QueryValue;

pub fn escape_preview_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
}

pub fn build_pk_pairs(
    columns: &[String],
    row: &[QueryValue],
    pk_columns: &[String],
) -> Option<Vec<(String, QueryValue)>> {
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

    mod value_preview {
        use super::*;

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
            let row = vec![QueryValue::text("1"), QueryValue::text("alice")];
            let pairs = build_pk_pairs(&columns, &row, &["id".to_string()]).unwrap();
            assert_eq!(pairs, vec![("id".to_string(), QueryValue::text("1"))]);
        }

        #[test]
        fn missing_pk_column_returns_none() {
            let columns = vec!["name".to_string()];
            let row = vec![QueryValue::text("alice")];
            let pairs = build_pk_pairs(&columns, &row, &["id".to_string()]);
            assert!(pairs.is_none());
        }
    }
}
