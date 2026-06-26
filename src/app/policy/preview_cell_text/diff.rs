use super::handling::PreviewCellTextHandling;

fn normalize_jsonb_for_diff(value: &str) -> String {
    serde_json::from_str::<serde_json::Value>(value)
        .and_then(|v| serde_json::to_string(&v))
        .unwrap_or_else(|_| value.to_string())
}

pub fn normalize_for_write_diff(value: &str, handling: PreviewCellTextHandling) -> String {
    match handling {
        PreviewCellTextHandling::PostgreSqlJsonb => normalize_jsonb_for_diff(value),
        PreviewCellTextHandling::RawText
        | PreviewCellTextHandling::PostgreSqlJsonLikeText
        | PreviewCellTextHandling::PostgreSqlJson => value.to_string(),
    }
}

pub fn uses_structured_json_diff(handling: PreviewCellTextHandling) -> bool {
    handling == PreviewCellTextHandling::PostgreSqlJsonb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DatabaseType;

    use super::super::handling::preview_cell_text_handling;

    #[test]
    fn jsonb_column_normalizes_key_order() {
        let handling = preview_cell_text_handling(DatabaseType::PostgreSQL, "jsonb");
        let pg_style = r#"{"industries": ["tech"], "company_size": "enterprise"}"#;
        let serde_style = r#"{"company_size":"enterprise","industries":["tech"]}"#;
        assert_eq!(
            normalize_for_write_diff(pg_style, handling),
            normalize_for_write_diff(serde_style, handling)
        );
    }

    #[test]
    fn text_column_preserves_json_like_string() {
        let handling = preview_cell_text_handling(DatabaseType::PostgreSQL, "text");
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
        let handling = preview_cell_text_handling(DatabaseType::SQLite, "TEXT");
        let original = r#"{"items":["admin","writer"]}"#;
        assert_eq!(normalize_for_write_diff(original, handling), original);
    }

    #[test]
    fn sqlite_jsonb_declared_type_stays_raw() {
        let handling = preview_cell_text_handling(DatabaseType::SQLite, "jsonb");
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
