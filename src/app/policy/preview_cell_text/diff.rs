use super::handling::PreviewCellTextDiffHandling;

fn normalize_json_for_diff(value: &str) -> String {
    normalize_structured_json_for_write(value)
        .ok()
        .unwrap_or_else(|| value.to_string())
}

pub(crate) fn normalize_structured_json_for_write(
    value: &str,
) -> Result<String, serde_json::Error> {
    serde_json::from_str::<serde_json::Value>(value).and_then(|value| serde_json::to_string(&value))
}

pub(crate) fn normalize_for_write_diff(
    value: &str,
    handling: PreviewCellTextDiffHandling,
) -> String {
    match handling {
        PreviewCellTextDiffHandling::StructuredJson => normalize_json_for_diff(value),
        PreviewCellTextDiffHandling::RawText => value.to_string(),
    }
}

pub(crate) fn uses_structured_json_diff(handling: PreviewCellTextDiffHandling) -> bool {
    handling == PreviewCellTextDiffHandling::StructuredJson
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DatabaseType;
    use rstest::rstest;

    use super::super::handling::CellPresentationPolicy;

    #[rstest]
    #[case(
        DatabaseType::PostgreSQL,
        "jsonb",
        r#"{"industries": ["tech"], "company_size": "enterprise"}"#,
        r#"{"company_size":"enterprise","industries":["tech"]}"#
    )]
    #[case(DatabaseType::MySQL, "json", r#"{"a": 1, "b": 2}"#, r#"{"b":2,"a":1}"#)]
    fn json_columns_normalize_key_order(
        #[case] database_type: DatabaseType,
        #[case] column_data_type: &str,
        #[case] first: &str,
        #[case] second: &str,
    ) {
        let handling =
            CellPresentationPolicy::new(database_type, column_data_type, "").diff_handling();

        assert_eq!(
            normalize_for_write_diff(first, handling),
            normalize_for_write_diff(second, handling)
        );
        assert!(uses_structured_json_diff(handling));
    }

    #[test]
    fn text_column_preserves_json_like_string() {
        let handling =
            CellPresentationPolicy::new(DatabaseType::PostgreSQL, "text", "").diff_handling();
        let spaced = r#"{ "a": 1 }"#;
        let compact = r#"{"a":1}"#;
        assert_eq!(normalize_for_write_diff(spaced, handling), spaced);
        assert_ne!(
            normalize_for_write_diff(spaced, handling),
            normalize_for_write_diff(compact, handling)
        );
    }

    #[test]
    fn sqlite_text_column_preserves_json_like_string() {
        let handling =
            CellPresentationPolicy::new(DatabaseType::SQLite, "TEXT", "").diff_handling();
        let original = r#"{"items":["admin","writer"]}"#;
        assert_eq!(normalize_for_write_diff(original, handling), original);
    }

    #[test]
    fn sqlite_json_declared_type_stays_raw() {
        let handling =
            CellPresentationPolicy::new(DatabaseType::SQLite, "jsonb", "").diff_handling();
        let pg_style = r#"{"industries": ["tech"], "company_size": "enterprise"}"#;
        let serde_style = r#"{"company_size":"enterprise","industries":["tech"]}"#;
        assert_eq!(
            normalize_for_write_diff(pg_style, handling),
            pg_style.to_string()
        );
        assert_ne!(
            normalize_for_write_diff(pg_style, handling),
            normalize_for_write_diff(serde_style, handling)
        );
    }
}
