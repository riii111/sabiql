use crate::domain::Table;

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quote_literal_sql(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn generate_ddl(table: &Table) -> String {
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

        if i < table.columns.len() - 1 {
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
            quote_literal_sql(comment)
        ));
    }

    for col in &table.columns {
        if let Some(comment) = &col.comment {
            ddl.push_str(&format!(
                "\n\nCOMMENT ON COLUMN {}.{} IS {};",
                qualified,
                quote_ident(&col.name),
                quote_literal_sql(comment)
            ));
        }
    }

    ddl
}

pub fn generate_ddl_postgres(table: &Table) -> String {
    generate_ddl(table)
}

pub fn ddl_line_count_postgres(table: &Table) -> usize {
    generate_ddl(table).lines().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Column;

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

    mod generate_ddl_postgres {
        use super::*;

        #[test]
        fn table_with_pk_returns_valid_ddl() {
            let table = make_table(
                vec![
                    make_column("id", "integer", false),
                    make_column("name", "text", true),
                ],
                Some(vec!["id".to_string()]),
            );

            let ddl = generate_ddl_postgres(&table);

            assert!(ddl.contains("CREATE TABLE \"public\".\"test_table\""));
            assert!(ddl.contains("\"id\" integer NOT NULL"));
            assert!(ddl.contains("\"name\" text"));
            assert!(ddl.contains("PRIMARY KEY (\"id\")"));
        }
    }

    mod comment_on_statements {
        use super::*;

        #[test]
        fn table_comment_appended_after_create() {
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("User accounts".to_string());

            let ddl = generate_ddl_postgres(&table);

            assert!(ddl.contains("COMMENT ON TABLE \"public\".\"test_table\" IS 'User accounts';"));
        }

        #[test]
        fn column_comment_appended_after_create() {
            let mut col = make_column("id", "integer", false);
            col.comment = Some("Primary key".to_string());
            let table = make_table(vec![col], None);

            let ddl = generate_ddl_postgres(&table);

            assert!(
                ddl.contains(
                    "COMMENT ON COLUMN \"public\".\"test_table\".\"id\" IS 'Primary key';"
                )
            );
        }

        #[test]
        fn single_quote_in_comment_is_escaped() {
            let mut table = make_table(vec![make_column("id", "integer", false)], None);
            table.comment = Some("It's a test".to_string());

            let ddl = generate_ddl_postgres(&table);

            assert!(ddl.contains("IS 'It''s a test';"));
        }

        #[test]
        fn no_comment_on_when_absent() {
            let table = make_table(vec![make_column("id", "integer", false)], None);

            let ddl = generate_ddl_postgres(&table);

            assert!(!ddl.contains("COMMENT ON"));
        }
    }

    mod ddl_line_count_postgres {
        use super::*;

        #[test]
        fn count_matches_actual_lines() {
            let table = make_table(vec![make_column("col", "text", true)], None);

            let ddl = generate_ddl_postgres(&table);
            let count = ddl_line_count_postgres(&table);

            assert_eq!(count, ddl.lines().count());
        }
    }
}
