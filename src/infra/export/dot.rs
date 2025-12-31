use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Result, eyre};

use crate::domain::Table;

/// Lightweight FK info for ER diagram generation (avoids cloning full ForeignKey)
#[derive(Debug, Clone)]
pub struct ErFkInfo {
    pub name: String,
    pub from_qualified: String,
    pub to_qualified: String,
}

/// Lightweight table info for ER diagram generation (avoids cloning full Table)
#[derive(Debug, Clone)]
pub struct ErTableInfo {
    pub qualified_name: String,
    pub name: String,
    pub schema: String,
    pub foreign_keys: Vec<ErFkInfo>,
}

impl ErTableInfo {
    pub fn from_table(qualified_name: &str, table: &Table) -> Self {
        Self {
            qualified_name: qualified_name.to_string(),
            name: table.name.clone(),
            schema: table.schema.clone(),
            foreign_keys: table
                .foreign_keys
                .iter()
                .map(|fk| ErFkInfo {
                    name: fk.name.clone(),
                    from_qualified: format!("{}.{}", fk.from_schema, fk.from_table),
                    to_qualified: fk.referenced_table(),
                })
                .collect(),
        }
    }
}

pub struct DotExporter;

impl DotExporter {
    fn escape_dot_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }

    /// Generate DOT for full database ER diagram (all tables and FKs)
    pub fn generate_full_dot(tables: &[ErTableInfo]) -> String {
        let mut dot = String::new();
        dot.push_str("digraph full_er {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=box, fontname=\"Helvetica\"];\n");
        dot.push_str("    edge [fontname=\"Helvetica\", fontsize=10];\n");
        dot.push('\n');

        // Sort by qualified name for stable output
        let mut sorted_tables: Vec<_> = tables.iter().collect();
        sorted_tables.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

        // Add all tables as nodes
        for table in &sorted_tables {
            let full_name = Self::escape_dot_string(&table.qualified_name);
            let table_name = Self::escape_dot_string(&table.name);
            let schema_name = Self::escape_dot_string(&table.schema);

            dot.push_str(&format!(
                "    \"{}\" [label=\"{}\\n({})\" style=filled fillcolor=lightblue];\n",
                full_name, table_name, schema_name
            ));
        }

        dot.push('\n');

        // Collect and sort all FK relationships for stable output
        let mut edges: Vec<_> = sorted_tables
            .iter()
            .flat_map(|table| {
                table.foreign_keys.iter().map(|fk| {
                    (
                        fk.from_qualified.clone(),
                        fk.to_qualified.clone(),
                        fk.name.clone(),
                    )
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
            .map_err(|_| eyre!("Graphviz (dot) not found. Please install Graphviz (e.g., brew install graphviz on macOS)"))?;

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

    fn create_test_tables() -> Vec<ErTableInfo> {
        vec![
            ErTableInfo {
                qualified_name: "public.users".to_string(),
                name: "users".to_string(),
                schema: "public".to_string(),
                foreign_keys: vec![],
            },
            ErTableInfo {
                qualified_name: "public.orders".to_string(),
                name: "orders".to_string(),
                schema: "public".to_string(),
                foreign_keys: vec![ErFkInfo {
                    name: "fk_user".to_string(),
                    from_qualified: "public.orders".to_string(),
                    to_qualified: "public.users".to_string(),
                }],
            },
            ErTableInfo {
                qualified_name: "public.products".to_string(),
                name: "products".to_string(),
                schema: "public".to_string(),
                foreign_keys: vec![],
            },
        ]
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
