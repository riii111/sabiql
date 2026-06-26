use crate::domain::DatabaseType;

use super::handling::PreviewCellTextHandling;

fn looks_like_json_container(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

pub fn format_for_cell_detail(
    value: &str,
    database_type: DatabaseType,
    handling: PreviewCellTextHandling,
) -> String {
    let should_pretty_print = match handling {
        PreviewCellTextHandling::PostgreSqlJson => true,
        PreviewCellTextHandling::PostgreSqlJsonb => false,
        PreviewCellTextHandling::RawText => {
            database_type != DatabaseType::SQLite && looks_like_json_container(value)
        }
    };
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
    use super::super::handling::preview_cell_text_handling;
    use super::*;

    #[test]
    fn postgresql_json_column_pretty_prints() {
        let handling = preview_cell_text_handling(DatabaseType::PostgreSQL, "json");
        let formatted =
            format_for_cell_detail(r#"{"b":2,"a":1}"#, DatabaseType::PostgreSQL, handling);
        assert_eq!(formatted, "{\n  \"a\": 1,\n  \"b\": 2\n}");
    }

    #[test]
    fn sqlite_text_json_container_stays_raw() {
        let handling = preview_cell_text_handling(DatabaseType::SQLite, "TEXT");
        let value = r#"{"items":["admin","writer"]}"#;
        assert_eq!(
            format_for_cell_detail(value, DatabaseType::SQLite, handling),
            value
        );
    }

    #[test]
    fn sqlite_json_declared_type_stays_raw() {
        let handling = preview_cell_text_handling(DatabaseType::SQLite, "json");
        let value = r#"{"b":2,"a":1}"#;
        assert_eq!(
            format_for_cell_detail(value, DatabaseType::SQLite, handling),
            value
        );
    }

    #[test]
    fn postgresql_text_json_container_pretty_prints() {
        let handling = preview_cell_text_handling(DatabaseType::PostgreSQL, "text");
        let value = r#"{"items":["admin","writer"]}"#;
        assert_eq!(
            format_for_cell_detail(value, DatabaseType::PostgreSQL, handling),
            "{\n  \"items\": [\n    \"admin\",\n    \"writer\"\n  ]\n}"
        );
    }
}
