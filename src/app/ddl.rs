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

    fn test_column(name: &str, data_type: &str, nullable: bool, is_primary_key: bool) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            is_primary_key,
            default: None,
            is_unique: false,
            comment: None,
            ordinal_position: 0,
        }
    }

    fn test_table(schema: &str, name: &str, columns: Vec<Column>, primary_key: Option<Vec<String>>) -> Table {
        Table {
            schema: schema.to_string(),
            name: name.to_string(),
            columns,
            primary_key,
            foreign_keys: vec![],
            indexes: vec![],
            rls: None,
            row_count_estimate: None,
            comment: None,
        }
    }

    #[test]
    fn generates_basic_ddl() {
        let table = test_table(
            "public",
            "users",
            vec![
                test_column("id", "integer", false, true),
                test_column("name", "text", true, false),
            ],
            Some(vec!["id".to_string()]),
        );

        let ddl = generate_ddl_postgres(&table);

        assert!(ddl.contains("CREATE TABLE \"public\".\"users\""));
        assert!(ddl.contains("\"id\" integer NOT NULL"));
        assert!(ddl.contains("\"name\" text"));
        assert!(ddl.contains("PRIMARY KEY (\"id\")"));
    }

    #[test]
    fn ddl_line_count_matches_lines() {
        let table = test_table(
            "public",
            "test",
            vec![test_column("col", "text", true, false)],
            None,
        );

        let ddl = generate_ddl_postgres(&table);
        let count = ddl_line_count_postgres(&table);

        assert_eq!(count, ddl.lines().count());
    }
}
