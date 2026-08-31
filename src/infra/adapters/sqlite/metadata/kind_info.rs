use super::super::sqlite3::virtual_table_module_name;
use crate::domain::{TableKind, TableKindInfo};

use super::super::sqlite3::metadata::RawTableKindInfo;

pub(super) fn table_kind_info_from_raw(raw: &RawTableKindInfo) -> TableKindInfo {
    table_kind_info_from_pragma(&raw.r#type, raw.wr, raw.strict, raw.sql.as_deref())
}

fn table_kind_info_from_pragma(
    table_type: &str,
    without_rowid: i64,
    strict: i64,
    sql: Option<&str>,
) -> TableKindInfo {
    let mut kind_info = TableKindInfo {
        kind: match table_type {
            "virtual" => TableKind::Virtual,
            "view" => TableKind::View,
            _ => TableKind::Table,
        },
        is_strict: strict != 0,
        without_rowid: without_rowid != 0,
        virtual_module: None,
    };
    if kind_info.kind == TableKind::Virtual {
        kind_info.virtual_module = sql.and_then(virtual_table_module_name);
    }
    kind_info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_virtual_module_from_ddl() {
        assert_eq!(
            virtual_table_module_name("CREATE VIRTUAL TABLE notes_fts USING fts5(body);"),
            Some("fts5".to_string())
        );
    }

    #[test]
    fn parses_virtual_module_when_using_starts_on_new_line() {
        assert_eq!(
            virtual_table_module_name("CREATE VIRTUAL TABLE notes_fts\nUSING fts5(body);"),
            Some("fts5".to_string())
        );
    }

    #[test]
    fn parses_virtual_module_after_quoted_table_name_containing_using() {
        assert_eq!(
            virtual_table_module_name(r#"CREATE VIRTUAL TABLE "using" USING fts5(body);"#),
            Some("fts5".to_string())
        );
    }

    #[test]
    fn parses_quoted_virtual_module_name() {
        assert_eq!(
            virtual_table_module_name(r#"CREATE VIRTUAL TABLE notes USING "fts5"(body);"#),
            Some("fts5".to_string())
        );
        assert_eq!(
            virtual_table_module_name("CREATE VIRTUAL TABLE notes USING [fts5](body);"),
            Some("fts5".to_string())
        );
    }

    #[test]
    fn table_name_containing_strict_does_not_mark_strict_when_pragma_is_zero() {
        let storage = table_kind_info_from_pragma(
            "table",
            0,
            0,
            Some("CREATE TABLE strict_users(id INTEGER PRIMARY KEY, name TEXT);"),
        );

        assert!(!storage.is_strict);
    }

    #[test]
    fn pragma_type_marks_view_kind() {
        let storage = table_kind_info_from_pragma(
            "view",
            0,
            0,
            Some("CREATE VIEW active_users AS SELECT id FROM users;"),
        );

        assert_eq!(storage.kind, TableKind::View);
        assert!(storage.virtual_module.is_none());
        assert!(!storage.is_strict);
        assert!(!storage.without_rowid);
    }

    #[test]
    fn default_literal_does_not_mark_virtual_when_pragma_type_is_table() {
        let storage = table_kind_info_from_pragma(
            "table",
            0,
            0,
            Some("CREATE TABLE docs(body TEXT DEFAULT 'create virtual table');"),
        );

        assert_eq!(storage.kind, TableKind::Table);
        assert!(storage.virtual_module.is_none());
    }

    #[test]
    fn pragma_fields_mark_strict_virtual_table() {
        let storage = table_kind_info_from_pragma(
            "virtual",
            0,
            0,
            Some("CREATE VIRTUAL TABLE notes_fts USING fts5(body);"),
        );

        assert_eq!(storage.kind, TableKind::Virtual);
        assert_eq!(storage.virtual_module.as_deref(), Some("fts5"));
    }
}
