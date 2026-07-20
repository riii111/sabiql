use crate::domain::DatabaseType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCellTextDiffHandling {
    RawText,
    PostgreSqlJsonb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCellTextDisplayHandling {
    RawText,
    SqliteText,
    PostgreSqlJsonLikeText,
    PostgreSqlJson,
    PostgreSqlJsonb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPresentationPolicy {
    diff_handling: PreviewCellTextDiffHandling,
    display_handling: PreviewCellTextDisplayHandling,
}

impl CellPresentationPolicy {
    pub fn new(database_type: DatabaseType, column_data_type: &str, value: &str) -> Self {
        let diff_handling = match (database_type, column_data_type) {
            (DatabaseType::PostgreSQL, "jsonb") => PreviewCellTextDiffHandling::PostgreSqlJsonb,
            _ => PreviewCellTextDiffHandling::RawText,
        };
        let display_handling = match database_type {
            DatabaseType::SQLite if has_sqlite_text_affinity(column_data_type) => {
                PreviewCellTextDisplayHandling::SqliteText
            }
            DatabaseType::SQLite => PreviewCellTextDisplayHandling::RawText,
            DatabaseType::PostgreSQL => match column_data_type {
                "jsonb" => PreviewCellTextDisplayHandling::PostgreSqlJsonb,
                "json" => PreviewCellTextDisplayHandling::PostgreSqlJson,
                _ if looks_like_json_container(value) => {
                    PreviewCellTextDisplayHandling::PostgreSqlJsonLikeText
                }
                _ => PreviewCellTextDisplayHandling::RawText,
            },
        };

        Self {
            diff_handling,
            display_handling,
        }
    }

    pub fn diff_handling(self) -> PreviewCellTextDiffHandling {
        self.diff_handling
    }

    pub fn display_handling(self) -> PreviewCellTextDisplayHandling {
        self.display_handling
    }

    pub fn uses_jsonb_detail_modal(self) -> bool {
        self.diff_handling == PreviewCellTextDiffHandling::PostgreSqlJsonb
    }
}

fn looks_like_json_container(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn has_sqlite_text_affinity(column_data_type: &str) -> bool {
    let upper = column_data_type.to_ascii_uppercase();
    upper.contains("CHAR") || upper.contains("CLOB") || upper.contains("TEXT")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_columns_always_use_raw_text() {
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::SQLite, "TEXT", "").diff_handling(),
            PreviewCellTextDiffHandling::RawText
        );
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::SQLite, "json", "").diff_handling(),
            PreviewCellTextDiffHandling::RawText
        );
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::SQLite, "jsonb", "").diff_handling(),
            PreviewCellTextDiffHandling::RawText
        );
        assert!(
            !CellPresentationPolicy::new(DatabaseType::SQLite, "jsonb", "")
                .uses_jsonb_detail_modal()
        );
    }

    #[test]
    fn postgresql_jsonb_uses_semantic_handling() {
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::PostgreSQL, "jsonb", "").diff_handling(),
            PreviewCellTextDiffHandling::PostgreSqlJsonb
        );
        assert!(
            CellPresentationPolicy::new(DatabaseType::PostgreSQL, "jsonb", "")
                .uses_jsonb_detail_modal()
        );
    }

    #[test]
    fn postgresql_json_uses_raw_text_for_diff() {
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::PostgreSQL, "json", "").diff_handling(),
            PreviewCellTextDiffHandling::RawText
        );
    }

    #[test]
    fn postgresql_text_uses_raw_text_for_diff() {
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::PostgreSQL, "text", "").diff_handling(),
            PreviewCellTextDiffHandling::RawText
        );
    }

    #[test]
    fn sqlite_text_affinity_uses_text_display_handling() {
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::SQLite, "TEXT", r#"{"items":["admin"]}"#)
                .display_handling(),
            PreviewCellTextDisplayHandling::SqliteText
        );
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::SQLite, "varchar(255)", "42")
                .display_handling(),
            PreviewCellTextDisplayHandling::SqliteText
        );
    }

    #[test]
    fn sqlite_non_text_affinity_uses_raw_display_handling() {
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::SQLite, "INTEGER", "42").display_handling(),
            PreviewCellTextDisplayHandling::RawText
        );
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::SQLite, "json", r#"{"a":1}"#)
                .display_handling(),
            PreviewCellTextDisplayHandling::RawText
        );
    }

    #[test]
    fn postgresql_text_json_container_uses_json_like_display_handling() {
        assert_eq!(
            CellPresentationPolicy::new(DatabaseType::PostgreSQL, "text", r#"{"items":["admin"]}"#)
                .display_handling(),
            PreviewCellTextDisplayHandling::PostgreSqlJsonLikeText
        );
    }
}
