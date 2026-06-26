use crate::domain::DatabaseType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCellTextHandling {
    RawText,
    PostgreSqlJsonLikeText,
    PostgreSqlJson,
    PostgreSqlJsonb,
}

pub fn preview_cell_text_handling(
    database_type: DatabaseType,
    column_data_type: &str,
) -> PreviewCellTextHandling {
    if database_type == DatabaseType::SQLite {
        return PreviewCellTextHandling::RawText;
    }

    match database_type {
        DatabaseType::PostgreSQL => match column_data_type {
            "jsonb" => PreviewCellTextHandling::PostgreSqlJsonb,
            "json" => PreviewCellTextHandling::PostgreSqlJson,
            _ => PreviewCellTextHandling::RawText,
        },
        DatabaseType::SQLite => PreviewCellTextHandling::RawText,
    }
}

pub fn preview_cell_text_display_handling(
    database_type: DatabaseType,
    column_data_type: &str,
    value: &str,
) -> PreviewCellTextHandling {
    match preview_cell_text_handling(database_type, column_data_type) {
        handling @ (PreviewCellTextHandling::PostgreSqlJson
        | PreviewCellTextHandling::PostgreSqlJsonb
        | PreviewCellTextHandling::PostgreSqlJsonLikeText) => handling,
        PreviewCellTextHandling::RawText
            if database_type != DatabaseType::SQLite && looks_like_json_container(value) =>
        {
            PreviewCellTextHandling::PostgreSqlJsonLikeText
        }
        PreviewCellTextHandling::RawText => PreviewCellTextHandling::RawText,
    }
}

pub fn uses_jsonb_detail_modal(handling: PreviewCellTextHandling) -> bool {
    handling == PreviewCellTextHandling::PostgreSqlJsonb
}

fn looks_like_json_container(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_columns_always_use_raw_text() {
        assert_eq!(
            preview_cell_text_handling(DatabaseType::SQLite, "TEXT"),
            PreviewCellTextHandling::RawText
        );
        assert_eq!(
            preview_cell_text_handling(DatabaseType::SQLite, "json"),
            PreviewCellTextHandling::RawText
        );
        assert_eq!(
            preview_cell_text_handling(DatabaseType::SQLite, "jsonb"),
            PreviewCellTextHandling::RawText
        );
        assert!(!uses_jsonb_detail_modal(preview_cell_text_handling(
            DatabaseType::SQLite,
            "jsonb"
        )));
    }

    #[test]
    fn postgresql_jsonb_uses_semantic_handling() {
        assert_eq!(
            preview_cell_text_handling(DatabaseType::PostgreSQL, "jsonb"),
            PreviewCellTextHandling::PostgreSqlJsonb
        );
        assert!(uses_jsonb_detail_modal(preview_cell_text_handling(
            DatabaseType::PostgreSQL,
            "jsonb"
        )));
    }

    #[test]
    fn postgresql_json_uses_json_display_handling() {
        assert_eq!(
            preview_cell_text_handling(DatabaseType::PostgreSQL, "json"),
            PreviewCellTextHandling::PostgreSqlJson
        );
    }

    #[test]
    fn postgresql_text_uses_raw_text_for_diff() {
        assert_eq!(
            preview_cell_text_handling(DatabaseType::PostgreSQL, "text"),
            PreviewCellTextHandling::RawText
        );
    }

    #[test]
    fn postgresql_text_json_container_uses_json_like_display_handling() {
        assert_eq!(
            preview_cell_text_display_handling(
                DatabaseType::PostgreSQL,
                "text",
                r#"{"items":["admin"]}"#
            ),
            PreviewCellTextHandling::PostgreSqlJsonLikeText
        );
    }
}
