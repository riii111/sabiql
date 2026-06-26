use crate::domain::DatabaseType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCellTextDiffHandling {
    RawText,
    PostgreSqlJsonb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCellTextDisplayHandling {
    RawText,
    PostgreSqlJsonLikeText,
    PostgreSqlJson,
    PostgreSqlJsonb,
}

pub fn preview_cell_text_diff_handling(
    database_type: DatabaseType,
    column_data_type: &str,
) -> PreviewCellTextDiffHandling {
    match (database_type, column_data_type) {
        (DatabaseType::PostgreSQL, "jsonb") => PreviewCellTextDiffHandling::PostgreSqlJsonb,
        _ => PreviewCellTextDiffHandling::RawText,
    }
}

pub fn preview_cell_text_display_handling(
    database_type: DatabaseType,
    column_data_type: &str,
    value: &str,
) -> PreviewCellTextDisplayHandling {
    match database_type {
        DatabaseType::SQLite => PreviewCellTextDisplayHandling::RawText,
        DatabaseType::PostgreSQL => match column_data_type {
            "jsonb" => PreviewCellTextDisplayHandling::PostgreSqlJsonb,
            "json" => PreviewCellTextDisplayHandling::PostgreSqlJson,
            _ if looks_like_json_container(value) => {
                PreviewCellTextDisplayHandling::PostgreSqlJsonLikeText
            }
            _ => PreviewCellTextDisplayHandling::RawText,
        },
    }
}

pub fn uses_jsonb_detail_modal(diff_handling: PreviewCellTextDiffHandling) -> bool {
    diff_handling == PreviewCellTextDiffHandling::PostgreSqlJsonb
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
            preview_cell_text_diff_handling(DatabaseType::SQLite, "TEXT"),
            PreviewCellTextDiffHandling::RawText
        );
        assert_eq!(
            preview_cell_text_diff_handling(DatabaseType::SQLite, "json"),
            PreviewCellTextDiffHandling::RawText
        );
        assert_eq!(
            preview_cell_text_diff_handling(DatabaseType::SQLite, "jsonb"),
            PreviewCellTextDiffHandling::RawText
        );
        assert!(!uses_jsonb_detail_modal(preview_cell_text_diff_handling(
            DatabaseType::SQLite,
            "jsonb"
        )));
    }

    #[test]
    fn postgresql_jsonb_uses_semantic_handling() {
        assert_eq!(
            preview_cell_text_diff_handling(DatabaseType::PostgreSQL, "jsonb"),
            PreviewCellTextDiffHandling::PostgreSqlJsonb
        );
        assert!(uses_jsonb_detail_modal(preview_cell_text_diff_handling(
            DatabaseType::PostgreSQL,
            "jsonb"
        )));
    }

    #[test]
    fn postgresql_json_uses_raw_text_for_diff() {
        assert_eq!(
            preview_cell_text_diff_handling(DatabaseType::PostgreSQL, "json"),
            PreviewCellTextDiffHandling::RawText
        );
    }

    #[test]
    fn postgresql_text_uses_raw_text_for_diff() {
        assert_eq!(
            preview_cell_text_diff_handling(DatabaseType::PostgreSQL, "text"),
            PreviewCellTextDiffHandling::RawText
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
            PreviewCellTextDisplayHandling::PostgreSqlJsonLikeText
        );
    }
}
