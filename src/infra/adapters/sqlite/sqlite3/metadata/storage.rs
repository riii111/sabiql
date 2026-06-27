use super::super::parser::lexer::{is_create_virtual_table_prefix, virtual_table_module_name};
use crate::domain::{TableObjectKind, TableStorage};

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct RawTableStorage {
    #[serde(rename = "type", default)]
    r#type: String,
    #[serde(default)]
    wr: i64,
    #[serde(default)]
    strict: i64,
    sql: Option<String>,
}

impl RawTableStorage {
    pub(super) fn into_table_storage(self) -> TableStorage {
        table_storage_from_pragma(&self.r#type, self.wr, self.strict, self.sql.as_deref())
    }
}

pub(super) fn table_storage_from_pragma(
    table_type: &str,
    without_rowid: i64,
    strict: i64,
    sql: Option<&str>,
) -> TableStorage {
    let mut storage = TableStorage {
        kind: if table_type == "virtual" {
            TableObjectKind::Virtual
        } else {
            TableObjectKind::Table
        },
        is_strict: strict != 0,
        without_rowid: without_rowid != 0,
        virtual_module: None,
    };
    if storage.kind == TableObjectKind::Virtual {
        storage.virtual_module = sql.and_then(virtual_table_module_name);
    }
    storage
}

pub(super) fn table_storage_from_legacy_sql(sql: Option<&str>) -> TableStorage {
    let mut storage = TableStorage::default();
    enrich_kind_from_legacy_sql(&mut storage, sql);
    if let Some(sql) = sql {
        let (is_strict, without_rowid) = parse_table_tail_options(sql);
        storage.is_strict = is_strict;
        storage.without_rowid = without_rowid;
    }
    storage
}

fn enrich_kind_from_legacy_sql(storage: &mut TableStorage, sql: Option<&str>) {
    let Some(sql) = sql else {
        return;
    };
    if is_create_virtual_table_prefix(sql) {
        storage.kind = TableObjectKind::Virtual;
        storage.virtual_module = virtual_table_module_name(sql);
    }
}

fn parse_table_tail_options(sql: &str) -> (bool, bool) {
    let normalized = normalize_ddl_whitespace(sql);
    let upper = normalized.to_ascii_uppercase();
    let Some(paren_end) = upper.rfind(')') else {
        return parse_option_tokens(&upper);
    };
    parse_option_tokens(&upper[paren_end + 1..])
}

fn strip_sql_comments(sql: &str) -> String {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                out.push(char::from(bytes[i]));
                i += 1;
                while i < bytes.len() {
                    out.push(char::from(bytes[i]));
                    if bytes[i] == quote {
                        if i + 1 < bytes.len() && bytes[i + 1] == quote {
                            i += 1;
                            out.push(char::from(bytes[i]));
                        } else {
                            i += 1;
                            break;
                        }
                    }
                    i += 1;
                }
            }
            b'[' => {
                out.push(char::from(bytes[i]));
                i += 1;
                while i < bytes.len() {
                    out.push(char::from(bytes[i]));
                    if bytes[i] == b']' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                out.push(' ');
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                out.push(' ');
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            byte => {
                out.push(char::from(byte));
                i += 1;
            }
        }
    }
    out
}

fn normalize_ddl_whitespace(sql: &str) -> String {
    strip_sql_comments(sql)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_option_tokens(tail: &str) -> (bool, bool) {
    let tail = tail.trim().trim_end_matches(';').trim();
    let tokens: Vec<&str> = tail
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
        .collect();
    let mut is_strict = false;
    let mut without_rowid = false;
    let mut idx = 0;
    while idx < tokens.len() {
        if tokens[idx].eq_ignore_ascii_case("STRICT") {
            is_strict = true;
        } else if tokens[idx].eq_ignore_ascii_case("WITHOUT")
            && tokens
                .get(idx + 1)
                .is_some_and(|token| token.eq_ignore_ascii_case("ROWID"))
        {
            without_rowid = true;
            idx += 1;
        }
        idx += 1;
    }
    (is_strict, without_rowid)
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
        let storage = table_storage_from_pragma(
            "table",
            0,
            0,
            Some("CREATE TABLE strict_users(id INTEGER PRIMARY KEY, name TEXT);"),
        );

        assert!(!storage.is_strict);
    }

    #[test]
    fn default_literal_does_not_mark_virtual_when_pragma_type_is_table() {
        let storage = table_storage_from_pragma(
            "table",
            0,
            0,
            Some("CREATE TABLE docs(body TEXT DEFAULT 'create virtual table');"),
        );

        assert_eq!(storage.kind, TableObjectKind::Table);
        assert!(storage.virtual_module.is_none());
    }

    #[test]
    fn legacy_sql_parses_without_rowid_from_table_tail() {
        let storage = table_storage_from_legacy_sql(Some(
            "CREATE TABLE settings(key TEXT PRIMARY KEY) WITHOUT ROWID;",
        ));

        assert!(storage.without_rowid);
        assert_eq!(storage.kind, TableObjectKind::Table);
    }

    #[test]
    fn legacy_sql_parses_strict_from_table_tail_only() {
        let storage = table_storage_from_legacy_sql(Some(
            "CREATE TABLE users(id INTEGER PRIMARY KEY) STRICT;",
        ));

        assert!(storage.is_strict);
    }

    #[test]
    fn legacy_sql_ignores_sql_comments_in_table_tail_options() {
        let without_rowid = table_storage_from_legacy_sql(Some(
            "CREATE TABLE settings(key TEXT PRIMARY KEY) WITHOUT /* c */ ROWID;",
        ));
        let strict = table_storage_from_legacy_sql(Some(
            "CREATE TABLE users(id INTEGER PRIMARY KEY) ) /* WITHOUT ROWID */ STRICT;",
        ));

        assert!(without_rowid.without_rowid);
        assert!(strict.is_strict);
        assert!(!strict.without_rowid);
    }

    #[test]
    fn legacy_sql_does_not_treat_comment_markers_inside_string_literals_as_options() {
        let storage = table_storage_from_legacy_sql(Some(
            "CREATE TABLE docs(body TEXT DEFAULT '/* not strict */') STRICT;",
        ));

        assert!(storage.is_strict);
    }

    #[test]
    fn legacy_sql_parses_comma_separated_table_options() {
        let strict_first = table_storage_from_legacy_sql(Some(
            "CREATE TABLE users(id INTEGER PRIMARY KEY) STRICT, WITHOUT ROWID;",
        ));
        let without_rowid_first = table_storage_from_legacy_sql(Some(
            "CREATE TABLE users(id INTEGER PRIMARY KEY) WITHOUT ROWID, STRICT;",
        ));

        assert!(strict_first.is_strict);
        assert!(strict_first.without_rowid);
        assert!(without_rowid_first.is_strict);
        assert!(without_rowid_first.without_rowid);
    }

    #[test]
    fn pragma_fields_mark_strict_virtual_table() {
        let storage = table_storage_from_pragma(
            "virtual",
            0,
            0,
            Some("CREATE VIRTUAL TABLE notes_fts USING fts5(body);"),
        );

        assert_eq!(storage.kind, TableObjectKind::Virtual);
        assert_eq!(storage.virtual_module.as_deref(), Some("fts5"));
    }
}
