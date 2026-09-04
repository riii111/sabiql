use crate::domain::{DatabaseType, TableKind, TableKindInfo, TableSummary};
use crate::model::shared::ui_state::text_display_width;

pub fn table_display_name(database_type: DatabaseType, schema: &str, name: &str) -> String {
    match database_type {
        DatabaseType::MySQL => name.to_string(),
        DatabaseType::PostgreSQL | DatabaseType::SQLite => format!("{schema}.{name}"),
    }
}

pub fn table_key_display_name(
    database_type: DatabaseType,
    database: Option<&str>,
    qualified_name: &str,
) -> String {
    if database_type != DatabaseType::MySQL {
        return qualified_name.to_string();
    }

    database
        .and_then(|database| {
            let qualified_database = qualified_name.get(..database.len())?;
            let suffix = qualified_name.get(database.len()..)?;
            qualified_database
                .eq_ignore_ascii_case(database)
                .then(|| suffix.strip_prefix('.'))
                .flatten()
        })
        .unwrap_or(qualified_name)
        .to_string()
}

pub fn explorer_table_label(summary: &TableSummary, database_type: DatabaseType) -> String {
    if database_type == DatabaseType::SQLite {
        summary.name.clone()
    } else {
        table_display_name(database_type, &summary.schema, &summary.name)
    }
}

pub fn explorer_table_label_width(summary: &TableSummary, database_type: DatabaseType) -> usize {
    text_display_width(&explorer_table_label(summary, database_type))
}

pub fn max_explorer_table_label_width<'a>(
    summaries: impl IntoIterator<Item = &'a TableSummary>,
    database_type: DatabaseType,
) -> usize {
    summaries
        .into_iter()
        .map(|summary| explorer_table_label_width(summary, database_type))
        .max()
        .unwrap_or(0)
}

pub fn inspector_kind_label(kind_info: &TableKindInfo) -> String {
    match (&kind_info.kind, &kind_info.virtual_module) {
        (TableKind::Virtual, Some(module)) => format!("Virtual table ({module})"),
        (TableKind::Virtual, None) => "Virtual table".to_string(),
        (TableKind::View, _) => "View".to_string(),
        (TableKind::Table, _) => "Table".to_string(),
    }
}

pub fn inspector_flags_label(kind_info: &TableKindInfo) -> Option<String> {
    let mut flags = Vec::new();
    if kind_info.is_strict {
        flags.push("STRICT");
    }
    if kind_info.without_rowid {
        flags.push("WITHOUT ROWID");
    }
    if flags.is_empty() {
        None
    } else {
        Some(flags.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_explorer_shows_table_name_without_schema() {
        let summary = TableSummary::new("main".to_string(), "users".to_string(), None, false);

        assert_eq!(
            explorer_table_label(&summary, DatabaseType::SQLite),
            "users"
        );
    }

    #[test]
    fn sqlite_explorer_hides_virtual_table_suffix_but_inspector_keeps_kind() {
        let summary = TableSummary::new("main".to_string(), "notes_fts".to_string(), None, false)
            .with_kind_info(TableKindInfo {
                kind: TableKind::Virtual,
                virtual_module: Some("fts5".to_string()),
                ..TableKindInfo::default()
            });

        assert_eq!(
            explorer_table_label(&summary, DatabaseType::SQLite),
            "notes_fts"
        );
        assert_eq!(
            inspector_kind_label(&summary.kind_info),
            "Virtual table (fts5)"
        );
    }

    #[test]
    fn sqlite_explorer_hides_table_flags_but_inspector_keeps_flags() {
        let summary = TableSummary::new("main".to_string(), "settings".to_string(), None, false)
            .with_kind_info(TableKindInfo {
                is_strict: true,
                without_rowid: true,
                ..TableKindInfo::default()
            });

        assert_eq!(
            explorer_table_label(&summary, DatabaseType::SQLite),
            "settings"
        );
        assert_eq!(
            inspector_flags_label(&summary.kind_info),
            Some("STRICT, WITHOUT ROWID".to_string())
        );
    }

    #[test]
    fn sqlite_explorer_hides_view_suffix_but_inspector_keeps_kind() {
        let summary =
            TableSummary::new("main".to_string(), "active_users".to_string(), None, false)
                .with_kind_info(TableKindInfo {
                    kind: TableKind::View,
                    ..TableKindInfo::default()
                });

        assert_eq!(
            explorer_table_label(&summary, DatabaseType::SQLite),
            "active_users"
        );
        assert_eq!(inspector_kind_label(&summary.kind_info), "View");
    }

    #[test]
    fn mysql_table_display_omits_database() {
        let summary = TableSummary::new("app".to_string(), "users".to_string(), None, false);

        assert_eq!(explorer_table_label(&summary, DatabaseType::MySQL), "users");
        assert_eq!(
            explorer_table_label(&summary, DatabaseType::PostgreSQL),
            "app.users"
        );
    }

    #[test]
    fn mysql_table_key_display_uses_table_name_without_changing_identity() {
        assert_eq!(
            table_key_display_name(DatabaseType::MySQL, Some("app"), "app.users"),
            "users"
        );
        assert_eq!(
            table_key_display_name(DatabaseType::MySQL, Some("APP"), "app.users"),
            "users"
        );
        assert_eq!(
            table_key_display_name(DatabaseType::MySQL, Some("app.db"), "app.db.users"),
            "users"
        );
        assert_eq!(
            table_key_display_name(DatabaseType::PostgreSQL, Some("app"), "app.users"),
            "app.users"
        );
    }
}
