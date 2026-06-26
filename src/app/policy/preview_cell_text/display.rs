use super::handling::PreviewCellTextDisplayHandling;

pub fn format_for_cell_detail(value: &str, handling: PreviewCellTextDisplayHandling) -> String {
    let should_pretty_print = matches!(
        handling,
        PreviewCellTextDisplayHandling::PostgreSqlJson
            | PreviewCellTextDisplayHandling::PostgreSqlJsonLikeText
    );
    if !should_pretty_print {
        return value.to_string();
    }

    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|json| serde_json::to_string_pretty(&json).ok())
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::handling::{
        PreviewCellTextDisplayHandling, preview_cell_text_display_handling,
    };
    use super::*;
    use crate::domain::DatabaseType;

    #[test]
    fn postgresql_json_column_pretty_prints() {
        let handling = preview_cell_text_display_handling(
            DatabaseType::PostgreSQL,
            "json",
            r#"{"b":2,"a":1}"#,
        );
        assert_eq!(handling, PreviewCellTextDisplayHandling::PostgreSqlJson);
        let formatted = format_for_cell_detail(r#"{"b":2,"a":1}"#, handling);
        assert_eq!(formatted, "{\n  \"a\": 1,\n  \"b\": 2\n}");
    }

    #[test]
    fn sqlite_text_json_container_stays_raw() {
        let handling = preview_cell_text_display_handling(
            DatabaseType::SQLite,
            "TEXT",
            r#"{"items":["admin","writer"]}"#,
        );
        let value = r#"{"items":["admin","writer"]}"#;
        assert_eq!(format_for_cell_detail(value, handling), value);
    }

    #[test]
    fn sqlite_json_declared_type_stays_raw() {
        let handling =
            preview_cell_text_display_handling(DatabaseType::SQLite, "json", r#"{"b":2,"a":1}"#);
        let value = r#"{"b":2,"a":1}"#;
        assert_eq!(format_for_cell_detail(value, handling), value);
    }

    #[test]
    fn sqlite_jsonb_declared_type_stays_raw() {
        let handling =
            preview_cell_text_display_handling(DatabaseType::SQLite, "jsonb", r#"{"b":2,"a":1}"#);
        let value = r#"{"b":2,"a":1}"#;
        assert_eq!(format_for_cell_detail(value, handling), value);
    }

    #[test]
    fn postgresql_text_json_container_pretty_prints() {
        let handling = preview_cell_text_display_handling(
            DatabaseType::PostgreSQL,
            "text",
            r#"{"items":["admin","writer"]}"#,
        );
        assert_eq!(
            handling,
            PreviewCellTextDisplayHandling::PostgreSqlJsonLikeText
        );
        assert_eq!(
            format_for_cell_detail(r#"{"items":["admin","writer"]}"#, handling),
            "{\n  \"items\": [\n    \"admin\",\n    \"writer\"\n  ]\n}"
        );
    }
}
