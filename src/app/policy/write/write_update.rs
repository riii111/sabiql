use crate::domain::QueryValue;

/// Whether diff comparison should treat values as PostgreSQL JSONB semantics.
pub fn uses_jsonb_semantic_diff(column_data_type: &str) -> bool {
    column_data_type == "jsonb"
}

/// Normalize a JSONB cell value for diff display.
/// Re-serialize valid JSON so before/after share key ordering and formatting.
pub fn normalize_for_diff(value: &str) -> String {
    serde_json::from_str::<serde_json::Value>(value)
        .and_then(|v| serde_json::to_string(&v))
        .unwrap_or_else(|_| value.to_string())
}

/// Normalize a cell value for write-preview diff display.
/// Only PostgreSQL JSONB columns use semantic JSON normalization; all other
/// columns keep the stored string representation (including SQLite TEXT).
pub fn normalize_cell_value_for_diff(column_data_type: &str, value: &str) -> String {
    if uses_jsonb_semantic_diff(column_data_type) {
        normalize_for_diff(value)
    } else {
        value.to_string()
    }
}

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

        #[test]
        fn json_with_different_key_order_returns_identical_output() {
            let pg_style = r#"{"industries": ["tech"], "company_size": "enterprise"}"#;
            let serde_style = r#"{"company_size":"enterprise","industries":["tech"]}"#;
            assert_eq!(
                normalize_for_diff(pg_style),
                normalize_for_diff(serde_style)
            );
        }

        #[test]
        fn non_json_value_returns_unchanged() {
            assert_eq!(normalize_for_diff("plain text"), "plain text");
            assert_eq!(normalize_for_diff("42"), "42");
        }
    }

    mod cell_diff_normalization {
        use super::*;

        #[test]
        fn jsonb_column_normalizes_key_order() {
            let pg_style = r#"{"industries": ["tech"], "company_size": "enterprise"}"#;
            let serde_style = r#"{"company_size":"enterprise","industries":["tech"]}"#;
            assert_eq!(
                normalize_cell_value_for_diff("jsonb", pg_style),
                normalize_cell_value_for_diff("jsonb", serde_style)
            );
        }

        #[test]
        fn text_column_preserves_json_like_string() {
            let spaced = r#"{ "a": 1 }"#;
            let compact = r#"{"a":1}"#;
            assert_eq!(normalize_cell_value_for_diff("text", spaced), spaced);
            assert_ne!(
                normalize_cell_value_for_diff("text", spaced),
                normalize_cell_value_for_diff("text", compact)
            );
        }

        #[test]
        fn sqlite_text_column_preserves_json_like_string() {
            let original = r#"{"items":["admin","writer"]}"#;
            assert_eq!(normalize_cell_value_for_diff("TEXT", original), original);
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
