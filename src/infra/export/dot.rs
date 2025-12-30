use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Result, eyre};

use crate::domain::Table;

pub struct DotExporter;

impl DotExporter {
    fn escape_dot_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }

    /// Generate DOT for full database ER diagram (all tables and FKs)
    pub fn generate_full_dot<'a, I>(tables: I) -> String
    where
        I: IntoIterator<Item = (&'a String, &'a Table)>,
    {
        let mut dot = String::new();
        dot.push_str("digraph full_er {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=box, fontname=\"Helvetica\"];\n");
        dot.push_str("    edge [fontname=\"Helvetica\", fontsize=10];\n");
        dot.push('\n');

        // Sort by qualified name for stable output
        let mut tables: Vec<_> = tables.into_iter().collect();
        tables.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Add all tables as nodes
        for (qualified_name, table) in &tables {
            let full_name = Self::escape_dot_string(qualified_name);
            let table_name = Self::escape_dot_string(&table.name);
            let schema_name = Self::escape_dot_string(&table.schema);

            dot.push_str(&format!(
                "    \"{}\" [label=\"{}\\n({})\" style=filled fillcolor=lightblue];\n",
                full_name, table_name, schema_name
            ));
        }

        dot.push('\n');

        // Collect and sort all FK relationships for stable output
        let mut edges: Vec<_> = tables
            .iter()
            .flat_map(|(_, table)| {
                table.foreign_keys.iter().map(|fk| {
                    let from = format!("{}.{}", fk.from_schema, fk.from_table);
                    let to = fk.referenced_table();
                    (from, to, fk.name.clone())
                })
            })
            .collect();
        edges.sort();

        // Add all FK relationships as edges
        for (from, to, label) in edges {
            let from_escaped = Self::escape_dot_string(&from);
            let to_escaped = Self::escape_dot_string(&to);
            let label_escaped = Self::escape_dot_string(&label);

            dot.push_str(&format!(
                "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
                from_escaped, to_escaped, label_escaped
            ));
        }

        dot.push_str("}\n");
        dot
    }

    pub fn export_dot_and_open(
        dot_content: &str,
        filename: &str,
        cache_dir: &Path,
    ) -> Result<PathBuf> {
        let dot_path = cache_dir.join(filename);
        std::fs::write(&dot_path, dot_content)?;

        let svg_path = dot_path.with_extension("svg");

        let status = Command::new("dot")
            .args(["-Tsvg", "-o"])
            .arg(&svg_path)
            .arg(&dot_path)
            .status()
            .map_err(|_| eyre!("Graphviz not found. Install with: brew install graphviz"))?;

        if !status.success() {
            return Err(eyre!(
                "Graphviz failed (exit code {:?}). Check DOT syntax.",
                status.code()
            ));
        }

        #[cfg(target_os = "macos")]
        {
            Command::new("open").arg(&svg_path).spawn()?;
        }
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open").arg(&svg_path).spawn()?;
        }
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/C", "start"])
                .arg(&svg_path)
                .spawn()?;
        }

        Ok(svg_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, FkAction, ForeignKey};
    use std::collections::HashMap;

    fn create_test_tables() -> HashMap<String, Table> {
        let mut tables = HashMap::new();

        tables.insert(
            "public.users".to_string(),
            Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                columns: vec![Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: false,
                    comment: None,
                    ordinal_position: 1,
                }],
                primary_key: Some(vec!["id".to_string()]),
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                row_count_estimate: Some(100),
                comment: None,
            },
        );

        tables.insert(
            "public.orders".to_string(),
            Table {
                schema: "public".to_string(),
                name: "orders".to_string(),
                columns: vec![],
                primary_key: None,
                foreign_keys: vec![ForeignKey {
                    name: "fk_user".to_string(),
                    from_schema: "public".to_string(),
                    from_table: "orders".to_string(),
                    from_columns: vec!["user_id".to_string()],
                    to_schema: "public".to_string(),
                    to_table: "users".to_string(),
                    to_columns: vec!["id".to_string()],
                    on_delete: FkAction::NoAction,
                    on_update: FkAction::NoAction,
                }],
                indexes: vec![],
                rls: None,
                row_count_estimate: Some(500),
                comment: None,
            },
        );

        tables.insert(
            "public.products".to_string(),
            Table {
                schema: "public".to_string(),
                name: "products".to_string(),
                columns: vec![],
                primary_key: None,
                foreign_keys: vec![],
                indexes: vec![],
                rls: None,
                row_count_estimate: Some(50),
                comment: None,
            },
        );

        tables
    }

    #[test]
    fn output_contains_all_tables_as_nodes() {
        let tables = create_test_tables();

        let dot = DotExporter::generate_full_dot(&tables);

        assert!(dot.contains("\"public.users\""));
        assert!(dot.contains("\"public.orders\""));
        assert!(dot.contains("\"public.products\""));
    }

    #[test]
    fn output_contains_fk_as_edge() {
        let tables = create_test_tables();

        let dot = DotExporter::generate_full_dot(&tables);

        assert!(dot.contains("\"public.orders\" -> \"public.users\""));
        assert!(dot.contains("label=\"fk_user\""));
    }

    #[test]
    fn output_uses_full_er_digraph_name() {
        let tables = create_test_tables();

        let dot = DotExporter::generate_full_dot(&tables);

        assert!(dot.contains("digraph full_er {"));
    }
}
