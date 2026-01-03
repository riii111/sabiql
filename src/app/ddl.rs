use crate::domain::Table;

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
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
            columns,
            primary_key,
            foreign_keys: vec![],
            indexes: vec![],
            rls: None,
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
