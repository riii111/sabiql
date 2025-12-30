use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Result, eyre};

use crate::domain::{NeighborhoodGraph, Table};

pub struct DotExporter;

impl DotExporter {
    fn escape_dot_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }

    pub fn sanitize_filename(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
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

        let tables: Vec<_> = tables.into_iter().collect();

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

        // Add all FK relationships as edges
        for (_, table) in &tables {
            for fk in &table.foreign_keys {
                let from = format!("{}.{}", fk.from_schema, fk.from_table);
                let to = fk.referenced_table();
                let from_escaped = Self::escape_dot_string(&from);
                let to_escaped = Self::escape_dot_string(&to);
                let label = Self::escape_dot_string(&fk.name);

                dot.push_str(&format!(
                    "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
                    from_escaped, to_escaped, label
                ));
            }
        }

        dot.push_str("}\n");
        dot
    }

    /// Generate DOT for neighborhood graph (centered on specific table)
    pub fn generate_dot(graph: &NeighborhoodGraph) -> String {
        let mut dot = String::new();

        dot.push_str("digraph neighborhood {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=box, fontname=\"Helvetica\"];\n");
        dot.push_str("    edge [fontname=\"Helvetica\", fontsize=10];\n");
        dot.push('\n');

        for node in &graph.nodes {
            let full_name = Self::escape_dot_string(&node.qualified_name());
            let table_name = Self::escape_dot_string(&node.table);
            let schema_name = Self::escape_dot_string(&node.schema);
            let color = if node.is_center() {
                "gold"
            } else {
                match node.hop_distance {
                    1 => "lightblue",
                    _ => "lightgray",
                }
            };

            dot.push_str(&format!(
                "    \"{}\" [label=\"{}\\n({})\" style=filled fillcolor={}];\n",
                full_name, table_name, schema_name, color
            ));
        }

        dot.push('\n');

        for edge in &graph.edges {
            let from = Self::escape_dot_string(&edge.from_node);
            let to = Self::escape_dot_string(&edge.to_node);
            let label = Self::escape_dot_string(&edge.fk_name);
            dot.push_str(&format!(
                "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
                from, to, label
            ));
        }

        dot.push_str("}\n");
        dot
    }

    pub fn export_to_file(graph: &NeighborhoodGraph, cache_dir: &Path) -> Result<PathBuf> {
        let dot_content = Self::generate_dot(graph);
        let safe_center = Self::sanitize_filename(&graph.center).replace('.', "_");
        let filename = format!("er_{}.dot", safe_center);
        let dot_path = cache_dir.join(&filename);

        std::fs::write(&dot_path, dot_content)?;
        Ok(dot_path)
    }

    pub fn export_and_open(graph: &NeighborhoodGraph, cache_dir: &Path) -> Result<PathBuf> {
        let dot_content = Self::generate_dot(graph);
        let safe_center = Self::sanitize_filename(&graph.center).replace('.', "_");
        let filename = format!("er_{}.dot", safe_center);
        Self::export_dot_and_open(&dot_content, &filename, cache_dir)
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
            .map_err(|e| eyre!("Failed to run 'dot' command: {}. Is Graphviz installed?", e))?;

        if !status.success() {
            return Err(eyre!(
                "dot command failed with exit code: {:?}",
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
    use crate::domain::{GraphEdge, GraphNode};

    fn create_test_graph() -> NeighborhoodGraph {
        let center = GraphNode::new("public".to_string(), "users".to_string(), 0);
        let related = GraphNode::new("public".to_string(), "orders".to_string(), 1);

        let edge = GraphEdge::new(
            "public.orders".to_string(),
            "public.users".to_string(),
            "fk_orders_user".to_string(),
            vec!["user_id".to_string()],
            vec!["id".to_string()],
        );

        let mut graph = NeighborhoodGraph::new("public.users".to_string(), 1);
        graph.nodes.push(center);
        graph.nodes.push(related);
        graph.edges.push(edge);
        graph
    }

    mod generate_dot {
        use super::*;

        #[test]
        fn output_contains_digraph_header() {
            let graph = create_test_graph();

            let dot = DotExporter::generate_dot(&graph);

            assert!(dot.contains("digraph neighborhood {"));
        }

        #[test]
        fn output_uses_left_to_right_direction() {
            let graph = create_test_graph();

            let dot = DotExporter::generate_dot(&graph);

            assert!(dot.contains("rankdir=LR"));
        }

        #[test]
        fn center_node_has_gold_fill_color() {
            let graph = create_test_graph();

            let dot = DotExporter::generate_dot(&graph);

            assert!(dot.contains("\"public.users\""));
            assert!(dot.contains("fillcolor=gold"));
        }

        #[test]
        fn one_hop_neighbor_has_lightblue_fill_color() {
            let graph = create_test_graph();

            let dot = DotExporter::generate_dot(&graph);

            assert!(dot.contains("\"public.orders\""));
            assert!(dot.contains("fillcolor=lightblue"));
        }

        #[test]
        fn edges_include_fk_name_as_label() {
            let graph = create_test_graph();

            let dot = DotExporter::generate_dot(&graph);

            assert!(dot.contains("\"public.orders\" -> \"public.users\""));
            assert!(dot.contains("label=\"fk_orders_user\""));
        }
    }

    mod export_to_file {
        use super::*;

        #[test]
        fn creates_file_with_dot_extension() {
            let graph = create_test_graph();
            let temp_dir = tempfile::tempdir().unwrap();

            let result = DotExporter::export_to_file(&graph, temp_dir.path());

            assert!(result.is_ok());
            let dot_path = result.unwrap();
            assert!(dot_path.exists());
            assert!(dot_path.extension().map_or(false, |ext| ext == "dot"));
        }
    }

    mod generate_full_dot {
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
}
