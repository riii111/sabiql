use crate::app::ports::DdlGenerator;
use crate::domain::Table;
use crate::infra::utils::{quote_ident, quote_literal};

use super::super::PostgresAdapter;

impl DdlGenerator for PostgresAdapter {
    fn generate_ddl(&self, table: &Table) -> String {
        let mut ddl = format!(
            "CREATE TABLE {}.{} (\n",
            quote_ident(&table.schema),
            quote_ident(&table.name)
        );

        for (i, col) in table.columns.iter().enumerate() {
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            let default = col
                .default
                .as_ref()
                .map(|d| format!(" DEFAULT {}", d))
                .unwrap_or_default();

            ddl.push_str(&format!(
                "  {} {}{}{}",
                quote_ident(&col.name),
                col.data_type,
                nullable,
                default
            ));

            if i + 1 < table.columns.len() {
                ddl.push(',');
            }
            ddl.push('\n');
        }

        if let Some(pk) = &table.primary_key {
            let quoted_cols: Vec<String> = pk.iter().map(|c| quote_ident(c)).collect();
            ddl.push_str(&format!("  PRIMARY KEY ({})\n", quoted_cols.join(", ")));
        }

        ddl.push_str(");");

        let qualified = format!(
            "{}.{}",
            quote_ident(&table.schema),
            quote_ident(&table.name)
        );

        if let Some(comment) = &table.comment {
            ddl.push_str(&format!(
                "\n\nCOMMENT ON TABLE {} IS {};",
                qualified,
                quote_literal(comment)
            ));
        }

        for col in &table.columns {
            if let Some(comment) = &col.comment {
                ddl.push_str(&format!(
                    "\n\nCOMMENT ON COLUMN {}.{} IS {};",
                    qualified,
                    quote_ident(&col.name),
                    quote_literal(comment)
                ));
            }
        }

        ddl
    }
}

#[cfg(test)]
mod tests {
    use crate::app::ports::DdlGenerator;
    use crate::domain::{Column, Table};
    use crate::infra::adapters::postgres::PostgresAdapter;

    fn make_column(name: &str, data_type: &str, nullable: bool) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            is_primary_key: false,
            default: None,
            is_unique: false,
            comment: None,
            ordinal_position: 0,
        }
    }

    fn make_table(columns: Vec<Column>, primary_key: Option<Vec<String>>) -> Table {
        Table {
            schema: "public".to_string(),
            name: "test_table".to_string(),
            owner: None,
            columns,
            primary_key,
            foreign_keys: vec![],
            indexes: vec![],
            rls: None,
            triggers: vec![],
            row_count_estimate: None,
            comment: None,
        }
    }

    mod ddl_generation {
        use super::*;

        #[test]
        fn table_with_pk_returns_valid_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(
                vec![
                    make_column("id", "integer", false),
                    make_column("name", "text", true),
                ],
                Some(vec!["id".to_string()]),
            );

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("CREATE TABLE \"public\".\"test_table\""));
            assert!(ddl.contains("\"id\" integer NOT NULL"));
            assert!(ddl.contains("\"name\" text"));
            assert!(ddl.contains("PRIMARY KEY (\"id\")"));
        }

        #[test]
        fn table_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("User accounts".to_string());

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("COMMENT ON TABLE \"public\".\"test_table\" IS 'User accounts';"));
        }

        #[test]
        fn column_comment_appended_after_create() {
            let adapter = PostgresAdapter::new();
            let mut col = make_column("id", "integer", false);
            col.comment = Some("Primary key".to_string());
            let table = make_table(vec![col], None);

            let ddl = adapter.generate_ddl(&table);

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

            let ddl = adapter.generate_ddl(&table);

            assert!(ddl.contains("IS 'It''s a test';"));
        }

        #[test]
        fn no_comment_on_when_absent() {
            let adapter = PostgresAdapter::new();
            let table = make_table(vec![make_column("id", "integer", false)], None);

            let ddl = adapter.generate_ddl(&table);

            assert!(!ddl.contains("COMMENT ON"));
        }

        #[test]
        fn default_ddl_line_count_matches_generated_ddl() {
            let adapter = PostgresAdapter::new();
            let table = make_table(vec![make_column("col", "text", true)], None);

            let ddl = adapter.generate_ddl(&table);
            let count = adapter.ddl_line_count(&table);

            assert_eq!(count, ddl.lines().count());
        }
    }
}
