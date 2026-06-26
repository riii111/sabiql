use crate::domain::DatabaseType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCellTextHandling {
    RawText,
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
    }

    #[test]
    fn postgresql_jsonb_uses_semantic_handling() {
        assert_eq!(
            preview_cell_text_handling(DatabaseType::PostgreSQL, "jsonb"),
            PreviewCellTextHandling::PostgreSqlJsonb
        );
    }

    #[test]
    fn postgresql_json_uses_json_display_handling() {
        assert_eq!(
            preview_cell_text_handling(DatabaseType::PostgreSQL, "json"),
            PreviewCellTextHandling::PostgreSqlJson
        );
    }

    #[test]
    fn postgresql_text_uses_raw_text() {
        assert_eq!(
            preview_cell_text_handling(DatabaseType::PostgreSQL, "text"),
            PreviewCellTextHandling::RawText
        );
    }
}
