use std::fmt::Write as _;

use crate::domain::dialect::{Dialect as _, Sqlite};
use crate::domain::{Table, Trigger};

use super::super::SqliteAdapter;

impl SqliteAdapter {
    pub fn generate_ddl(&self, table: &Table) -> String {
        if let Some(source_ddl) = table.source_ddl() {
            let mut ddl = source_ddl.to_string();
            append_trigger_ddls(&mut ddl, &table.triggers);
            return ddl;
        }

        let mut ddl = format!("CREATE TABLE {} (\n", Sqlite.quote_identifier(&table.name));
        let has_primary_key = table.primary_key.as_ref().is_some_and(|pk| !pk.is_empty());

        for (i, col) in table.columns.iter().enumerate() {
            let nullable = if col.is_nullable() { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map_or_default(|d| format!(" DEFAULT {d}"));

            let _ = write!(
                ddl,
                "  {} {}{}{}",
                Sqlite.quote_identifier(&col.name),
                col.data_type,
                nullable,
                default
            );

            if i + 1 < table.columns.len() || has_primary_key {
                ddl.push(',');
            }
            ddl.push('\n');
        }

        if let Some(pk) = &table.primary_key
            && !pk.is_empty()
        {
            let quoted_cols: Vec<String> = pk.iter().map(|c| Sqlite.quote_identifier(c)).collect();
            let _ = writeln!(ddl, "  PRIMARY KEY ({})", quoted_cols.join(", "));
        }

        ddl.push_str(");");
        append_trigger_ddls(&mut ddl, &table.triggers);
        ddl
    }
}

fn append_trigger_ddls(ddl: &mut String, triggers: &[Trigger]) {
    if triggers.is_empty() {
        return;
    }
    terminate_ddl_statement(ddl);
    for trigger in triggers {
        ddl.push('\n');
        ddl.push('\n');
        ddl.push_str(trigger.definition.trim());
        if !trigger.definition.trim_end().ends_with(';') {
            ddl.push(';');
        }
    }
}

fn terminate_ddl_statement(ddl: &mut String) {
    let trimmed_len = ddl.trim_end().len();
    if trimmed_len == 0 {
        ddl.clear();
        return;
    }
    ddl.truncate(trimmed_len);
    if !ddl.ends_with(';') {
        ddl.push(';');
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support;
    use crate::domain::{Column, ColumnAttributes, TriggerEvent, TriggerTiming};

    use super::*;

    fn make_column(name: &str, data_type: &str, nullable: bool) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            default: None,
            attributes: ColumnAttributes::from_parts(nullable, false, false),
            comment: None,
            ordinal_position: 0,
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        }
    }

    fn make_table(columns: Vec<Column>, primary_key: Option<Vec<String>>) -> Table {
        Table {
            schema: "main".to_string(),
            name: "test_table".to_string(),
            columns,
            primary_key,
            ..test_support::minimal_table("", "")
        }
    }

    mod ddl_generation {
        use super::*;

        #[test]
        fn table_with_pk_returns_schema_free_ddl() {
            let adapter = SqliteAdapter::new();
            let table = make_table(
                vec![
                    make_column("id", "INTEGER", false),
                    make_column("name", "TEXT", true),
                ],
                Some(vec!["id".to_string()]),
            );

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("CREATE TABLE \"test_table\""));
            assert!(ddl.contains("\"id\" INTEGER NOT NULL"));
            assert!(ddl.contains("\"name\" TEXT"));
            assert!(ddl.contains("PRIMARY KEY (\"id\")"));
            assert!(!ddl.contains("\"main\".\"test_table\""));
        }

        #[test]
        fn composite_primary_key_quotes_all_columns() {
            let adapter = SqliteAdapter::new();
            let table = make_table(
                vec![
                    make_column("tenant_id", "INTEGER", false),
                    make_column("id", "INTEGER", false),
                ],
                Some(vec!["tenant_id".to_string(), "id".to_string()]),
            );

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("PRIMARY KEY (\"tenant_id\", \"id\")"));
        }

        #[test]
        fn defaults_are_preserved_and_comments_are_omitted() {
            let adapter = SqliteAdapter::new();
            let mut column = make_column("created_at", "TEXT", false);
            column.default = Some("CURRENT_TIMESTAMP".to_string());
            column.comment = Some("created time".to_string());
            let mut table = make_table(vec![column], None);
            table.comment = Some("events".to_string());

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("\"created_at\" TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP"));
            assert!(!ddl.contains("COMMENT ON"));
        }

        #[test]
        fn source_ddl_appends_trigger_definitions() {
            let adapter = SqliteAdapter::new();
            let mut table = make_table(vec![make_column("id", "INTEGER", false)], None);
            table.source_ddl =
                Some("CREATE TABLE \"users\" (\n  \"id\" INTEGER NOT NULL\n)".to_string());
            table.triggers.push(Trigger {
                name: "users_audit".to_string(),
                timing: TriggerTiming::After,
                events: vec![TriggerEvent::Insert],
                action_order: None,
                definition: "CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END"
                    .to_string(),
                security_context: None,
                creation_context: None,
            });

            let ddl = adapter.generate_ddl(&table);

            assert_eq!(
                ddl,
                "CREATE TABLE \"users\" (\n  \"id\" INTEGER NOT NULL\n);\n\nCREATE TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END;"
            );
        }
    }
}
