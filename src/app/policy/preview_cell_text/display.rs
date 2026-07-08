use super::handling::PreviewCellTextDisplayHandling;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellDetailDisplay {
    pub content: String,
    pub formatted_json: bool,
}

pub fn format_for_cell_detail(
    value: &str,
    handling: PreviewCellTextDisplayHandling,
) -> CellDetailDisplay {
    let should_pretty_print = matches!(
        handling,
        PreviewCellTextDisplayHandling::SqliteText
            | PreviewCellTextDisplayHandling::PostgreSqlJson
            | PreviewCellTextDisplayHandling::PostgreSqlJsonLikeText
    );
    if !should_pretty_print {
        return CellDetailDisplay {
            content: value.to_string(),
            formatted_json: false,
        };
    }

    let Some(content) = serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|json| serde_json::to_string_pretty(&json).ok())
    else {
        return CellDetailDisplay {
            content: value.to_string(),
            formatted_json: false,
        };
    };

    CellDetailDisplay {
        content,
        formatted_json: true,
    }
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
        assert_eq!(formatted.content, "{\n  \"a\": 1,\n  \"b\": 2\n}");
        assert!(formatted.formatted_json);
    }

    #[test]
    fn sqlite_text_json_value_pretty_prints() {
        let handling = preview_cell_text_display_handling(
            DatabaseType::SQLite,
            "TEXT",
            r#"{"items":["admin","writer"]}"#,
        );
        assert_eq!(
            format_for_cell_detail(r#"{"items":["admin","writer"]}"#, handling),
            CellDetailDisplay {
                content: "{\n  \"items\": [\n    \"admin\",\n    \"writer\"\n  ]\n}".to_string(),
                formatted_json: true,
            }
        );
        assert_eq!(
            format_for_cell_detail(
                "42",
                preview_cell_text_display_handling(DatabaseType::SQLite, "TEXT", "42",)
            ),
            CellDetailDisplay {
                content: "42".to_string(),
                formatted_json: true,
            }
        );
    }

    #[test]
    fn sqlite_json_declared_type_stays_raw() {
        let handling =
            preview_cell_text_display_handling(DatabaseType::SQLite, "json", r#"{"b":2,"a":1}"#);
        let value = r#"{"b":2,"a":1}"#;
        assert_eq!(
            format_for_cell_detail(value, handling),
            CellDetailDisplay {
                content: value.to_string(),
                formatted_json: false,
            }
        );
    }

    #[test]
    fn sqlite_jsonb_declared_type_stays_raw() {
        let handling =
            preview_cell_text_display_handling(DatabaseType::SQLite, "jsonb", r#"{"b":2,"a":1}"#);
        let value = r#"{"b":2,"a":1}"#;
        assert_eq!(
            format_for_cell_detail(value, handling),
            CellDetailDisplay {
                content: value.to_string(),
                formatted_json: false,
            }
        );
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
            CellDetailDisplay {
                content: "{\n  \"items\": [\n    \"admin\",\n    \"writer\"\n  ]\n}".to_string(),
                formatted_json: true,
            }
        );
    }
}
