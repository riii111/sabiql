use crate::domain::{TableObjectKind, TableStorage, TableSummary};
use crate::model::shared::ui_state::text_display_width;

fn has_list_annotation(storage: &TableStorage) -> bool {
    storage.kind != TableObjectKind::Table
        || storage.is_strict
        || storage.without_rowid
        || storage.virtual_module.is_some()
}

pub fn explorer_storage_suffix(storage: &TableStorage) -> Option<String> {
    if !has_list_annotation(storage) {
        return None;
    }

    let mut parts = Vec::new();
    match storage.kind {
        TableObjectKind::Table => parts.push("table".to_string()),
        TableObjectKind::Virtual => {
            if let Some(module) = &storage.virtual_module {
                parts.push(format!("virtual/{module}"));
            } else {
                parts.push("virtual".to_string());
            }
        }
    }
    if storage.is_strict {
        parts.push("strict".to_string());
    }
    if storage.without_rowid {
        parts.push("no-rowid".to_string());
    }
    Some(format!(" [{}]", parts.join("+")))
}

pub fn explorer_table_label(summary: &TableSummary) -> String {
    let mut label = summary.qualified_name();
    if let Some(suffix) = explorer_storage_suffix(&summary.storage) {
        label.push_str(&suffix);
    }
    label
}

pub fn explorer_table_label_width(summary: &TableSummary) -> usize {
    text_display_width(&explorer_table_label(summary))
}

pub fn max_explorer_table_label_width<'a>(
    summaries: impl IntoIterator<Item = &'a TableSummary>,
) -> usize {
    summaries
        .into_iter()
        .map(explorer_table_label_width)
        .max()
        .unwrap_or(0)
}

pub fn inspector_kind_label(storage: &TableStorage) -> String {
    match (&storage.kind, &storage.virtual_module) {
        (TableObjectKind::Virtual, Some(module)) => format!("Virtual table ({module})"),
        (TableObjectKind::Virtual, None) => "Virtual table".to_string(),
        (TableObjectKind::Table, _) => "Table".to_string(),
    }
}

pub fn inspector_flags_label(storage: &TableStorage) -> Option<String> {
    let mut flags = Vec::new();
    if storage.is_strict {
        flags.push("STRICT");
    }
    if storage.without_rowid {
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
    fn ordinary_table_has_no_explorer_suffix() {
        let summary = TableSummary::new("main".to_string(), "users".to_string(), None, false);

        assert_eq!(explorer_storage_suffix(&summary.storage), None);
        assert_eq!(explorer_table_label(&summary), "main.users");
    }

    #[test]
    fn virtual_table_shows_module_in_explorer_suffix() {
        let summary = TableSummary::new("main".to_string(), "notes_fts".to_string(), None, false)
            .with_storage(TableStorage {
                kind: TableObjectKind::Virtual,
                virtual_module: Some("fts5".to_string()),
                ..TableStorage::default()
            });

        assert_eq!(
            explorer_storage_suffix(&summary.storage),
            Some(" [virtual/fts5]".to_string())
        );
        assert_eq!(
            explorer_table_label(&summary),
            "main.notes_fts [virtual/fts5]"
        );
        assert_eq!(
            inspector_kind_label(&summary.storage),
            "Virtual table (fts5)"
        );
    }

    #[test]
    fn strict_without_rowid_table_shows_flags() {
        let summary = TableSummary::new("main".to_string(), "settings".to_string(), None, false)
            .with_storage(TableStorage {
                is_strict: true,
                without_rowid: true,
                ..TableStorage::default()
            });

        assert_eq!(
            explorer_storage_suffix(&summary.storage),
            Some(" [table+strict+no-rowid]".to_string())
        );
        assert_eq!(
            inspector_flags_label(&summary.storage),
            Some("STRICT, WITHOUT ROWID".to_string())
        );
    }
}
