use crate::domain::{TableKind, TableKindInfo, TableSummary};
use crate::model::shared::ui_state::text_display_width;

fn has_list_annotation(kind_info: &TableKindInfo) -> bool {
    kind_info.kind != TableKind::Table
        || kind_info.is_strict
        || kind_info.without_rowid
        || kind_info.virtual_module.is_some()
}

pub fn explorer_kind_suffix(kind_info: &TableKindInfo) -> Option<String> {
    if !has_list_annotation(kind_info) {
        return None;
    }

    let mut parts = Vec::new();
    match kind_info.kind {
        TableKind::Table => parts.push("table".to_string()),
        TableKind::Virtual => {
            if let Some(module) = &kind_info.virtual_module {
                parts.push(format!("virtual/{module}"));
            } else {
                parts.push("virtual".to_string());
            }
        }
    }
    if kind_info.is_strict {
        parts.push("strict".to_string());
    }
    if kind_info.without_rowid {
        parts.push("no-rowid".to_string());
    }
    Some(format!(" [{}]", parts.join("+")))
}

pub fn explorer_table_label(summary: &TableSummary) -> String {
    let mut label = summary.qualified_name();
    if let Some(suffix) = explorer_kind_suffix(&summary.kind_info) {
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

pub fn inspector_kind_label(kind_info: &TableKindInfo) -> String {
    match (&kind_info.kind, &kind_info.virtual_module) {
        (TableKind::Virtual, Some(module)) => format!("Virtual table ({module})"),
        (TableKind::Virtual, None) => "Virtual table".to_string(),
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
    fn ordinary_table_has_no_explorer_suffix() {
        let summary = TableSummary::new("main".to_string(), "users".to_string(), None, false);

        assert_eq!(explorer_kind_suffix(&summary.kind_info), None);
        assert_eq!(explorer_table_label(&summary), "main.users");
    }

    #[test]
    fn virtual_table_shows_module_in_explorer_suffix() {
        let summary = TableSummary::new("main".to_string(), "notes_fts".to_string(), None, false)
            .with_kind_info(TableKindInfo {
                kind: TableKind::Virtual,
                virtual_module: Some("fts5".to_string()),
                ..TableKindInfo::default()
            });

        assert_eq!(
            explorer_kind_suffix(&summary.kind_info),
            Some(" [virtual/fts5]".to_string())
        );
        assert_eq!(
            explorer_table_label(&summary),
            "main.notes_fts [virtual/fts5]"
        );
        assert_eq!(
            inspector_kind_label(&summary.kind_info),
            "Virtual table (fts5)"
        );
    }

    #[test]
    fn strict_without_rowid_table_shows_flags() {
        let summary = TableSummary::new("main".to_string(), "settings".to_string(), None, false)
            .with_kind_info(TableKindInfo {
                is_strict: true,
                without_rowid: true,
                ..TableKindInfo::default()
            });

        assert_eq!(
            explorer_kind_suffix(&summary.kind_info),
            Some(" [table+strict+no-rowid]".to_string())
        );
        assert_eq!(
            inspector_flags_label(&summary.kind_info),
            Some("STRICT, WITHOUT ROWID".to_string())
        );
    }
}
