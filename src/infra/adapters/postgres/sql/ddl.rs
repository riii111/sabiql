use std::fmt::Write as _;

use crate::app::ports::outbound::DdlGenerator;
use crate::domain::{DatabaseType, Table};

use super::super::PostgresAdapter;
use super::{quote_ident, quote_literal};

impl DdlGenerator for PostgresAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
        let qualified = format!(
            "{}.{}",
            quote_ident(&table.schema),
            quote_ident(&table.name)
        );
        let mut ddl = format!("CREATE TABLE {qualified} (\n");

        let mut elements = Vec::with_capacity(table.columns.len() + 1);
        for col in &table.columns {
            let nullable = if col.is_nullable() { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map(|d| format!(" DEFAULT {d}"))
                .unwrap_or_default();

            elements.push(format!(
                "  {} {}{}{}",
                quote_ident(&col.name),
                col.data_type,
                nullable,
                default
            ));
        }

        if let Some(pk) = &table.primary_key {
            let quoted_cols: Vec<String> = pk.iter().map(|c| quote_ident(c)).collect();
            elements.push(format!("  PRIMARY KEY ({})", quoted_cols.join(", ")));
        }

        if !elements.is_empty() {
            ddl.push_str(&elements.join(",\n"));
            ddl.push('\n');
        }
        ddl.push_str(");");

        if let Some(comment) = &table.comment {
            let _ = write!(
                ddl,
                "\n\nCOMMENT ON TABLE {} IS {};",
                qualified,
                quote_literal(comment)
            );
        }

        for col in &table.columns {
            if let Some(comment) = &col.comment {
                let _ = write!(
                    ddl,
                    "\n\nCOMMENT ON COLUMN {}.{} IS {};",
                    qualified,
                    quote_ident(&col.name),
                    quote_literal(comment)
                );
            }
        }

        ddl
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::test_support;

    use crate::adapters::postgres::PostgresAdapter;
    use crate::app::ports::outbound::DdlGenerator;
    use crate::domain::{Column, ColumnAttributes, DatabaseType, Table};

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
            schema: "public".to_string(),
            name: "test_table".to_string(),
            columns,
            primary_key,
            ..test_support::minimal_table("", "")
        }
    }

    mod ddl_generation {
        use super::*;

        #[test]
        fn table_with_single_primary_key_returns_valid_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(
                vec![
                    make_column("id", "integer", false),
                    make_column("name", "text", true),
                ],
                Some(vec!["id".to_string()]),
            );

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert_eq!(
                ddl,
                "CREATE TABLE \"public\".\"test_table\" (\n  \"id\" integer NOT NULL,\n  \"name\" text,\n  PRIMARY KEY (\"id\")\n);"
            );
        }

        #[test]
        fn table_with_composite_primary_key_returns_valid_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(
                vec![
                    make_column("tenant_id", "integer", false),
                    make_column("id", "integer", false),
                ],
                Some(vec!["tenant_id".to_string(), "id".to_string()]),
            );

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert_eq!(
                ddl,
                "CREATE TABLE \"public\".\"test_table\" (\n  \"tenant_id\" integer NOT NULL,\n  \"id\" integer NOT NULL,\n  PRIMARY KEY (\"tenant_id\", \"id\")\n);"
            );
        }

        #[test]
        fn table_without_primary_key_returns_valid_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(
                vec![
                    make_column("id", "integer", false),
                    make_column("name", "text", true),
                ],
                None,
            );

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert_eq!(
                ddl,
                "CREATE TABLE \"public\".\"test_table\" (\n  \"id\" integer NOT NULL,\n  \"name\" text\n);"
            );
        }

        #[test]
        fn empty_table_returns_valid_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(Vec::new(), None);

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert_eq!(ddl, "CREATE TABLE \"public\".\"test_table\" (\n);");
        }

        #[test]
        fn table_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("User accounts".to_string());

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert!(ddl.contains("COMMENT ON TABLE \"public\".\"test_table\" IS 'User accounts';"));
        }

        #[test]
        fn column_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut col = make_column("id", "integer", false);
            col.comment = Some("Primary key".to_string());
            let table = make_table(vec![col], None);

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert!(
                ddl.contains(
                    "COMMENT ON COLUMN \"public\".\"test_table\".\"id\" IS 'Primary key';"
                )
            );
        }

        #[test]
        fn single_quote_in_comment_is_escaped() {
            let adapter = PostgresAdapter::new();
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("It's a test".to_string());

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert!(ddl.contains("IS 'It''s a test';"));
        }

        #[test]
        fn no_comment_on_when_absent() {
            let adapter = PostgresAdapter::new();
            let table = make_table(vec![make_column("id", "integer", false)], None);

            let ddl = adapter.generate_ddl(DatabaseType::PostgreSQL, &table);

            assert!(!ddl.contains("COMMENT ON"));
        }
    }
}
