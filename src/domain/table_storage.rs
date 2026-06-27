#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableObjectKind {
    #[default]
    Table,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableStorage {
    pub kind: TableObjectKind,
    pub is_strict: bool,
    pub without_rowid: bool,
    pub virtual_module: Option<String>,
}

impl TableStorage {
    pub fn has_list_annotation(&self) -> bool {
        self.kind != TableObjectKind::Table
            || self.is_strict
            || self.without_rowid
            || self.virtual_module.is_some()
    }

    pub fn explorer_suffix(&self) -> Option<String> {
        if !self.has_list_annotation() {
            return None;
        }

        let mut parts = Vec::new();
        match self.kind {
            TableObjectKind::Table => parts.push("table".to_string()),
            TableObjectKind::Virtual => {
                if let Some(module) = &self.virtual_module {
                    parts.push(format!("virtual/{module}"));
                } else {
                    parts.push("virtual".to_string());
                }
            }
        }
        if self.is_strict {
            parts.push("strict".to_string());
        }
        if self.without_rowid {
            parts.push("no-rowid".to_string());
        }
        Some(format!(" [{}]", parts.join("+")))
    }

    pub fn kind_detail(&self) -> String {
        match (&self.kind, &self.virtual_module) {
            (TableObjectKind::Virtual, Some(module)) => format!("Virtual table ({module})"),
            (TableObjectKind::Virtual, None) => "Virtual table".to_string(),
            (TableObjectKind::Table, _) => "Table".to_string(),
        }
    }

    pub fn flags_label(&self) -> Option<String> {
        let mut flags = Vec::new();
        if self.is_strict {
            flags.push("STRICT");
        }
        if self.without_rowid {
            flags.push("WITHOUT ROWID");
        }
        if flags.is_empty() {
            None
        } else {
            Some(flags.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_table_has_no_explorer_suffix() {
        let storage = TableStorage::default();

        assert!(!storage.has_list_annotation());
        assert_eq!(storage.explorer_suffix(), None);
    }

    #[test]
    fn virtual_table_shows_module_in_explorer_suffix() {
        let storage = TableStorage {
            kind: TableObjectKind::Virtual,
            virtual_module: Some("fts5".to_string()),
            ..TableStorage::default()
        };

        assert_eq!(
            storage.explorer_suffix(),
            Some(" [virtual/fts5]".to_string())
        );
        assert_eq!(storage.kind_detail(), "Virtual table (fts5)");
    }

    #[test]
    fn strict_without_rowid_table_shows_flags() {
        let storage = TableStorage {
            is_strict: true,
            without_rowid: true,
            ..TableStorage::default()
        };

        assert_eq!(
            storage.explorer_suffix(),
            Some(" [table+strict+no-rowid]".to_string())
        );
        assert_eq!(
            storage.flags_label(),
            Some("STRICT, WITHOUT ROWID".to_string())
        );
    }
}
