use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Result, eyre};

use crate::domain::NeighborhoodGraph;

pub struct DotExporter;

impl DotExporter {
    /// Generate DOT syntax from a neighborhood graph
    pub fn generate_dot(graph: &NeighborhoodGraph) -> String {
        let mut dot = String::new();

        dot.push_str("digraph neighborhood {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=box, fontname=\"Helvetica\"];\n");
        dot.push_str("    edge [fontname=\"Helvetica\", fontsize=10];\n");
        dot.push('\n');

        // Nodes with styling based on hop distance
        for node in &graph.nodes {
            let full_name = node.qualified_name();
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
                full_name, node.table, node.schema, color
            ));
        }

        dot.push('\n');

        // Edges with FK names (all edges drawn solid, direction is implicit in from->to)
        for edge in &graph.edges {
            dot.push_str(&format!(
                "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
                edge.from_node, edge.to_node, edge.fk_name
            ));
        }

        dot.push_str("}\n");
        dot
    }

    /// Export graph to DOT file in cache directory
    pub fn export_to_file(graph: &NeighborhoodGraph, cache_dir: &Path) -> Result<PathBuf> {
        let dot_content = Self::generate_dot(graph);
        let filename = format!("er_{}.dot", graph.center.replace('.', "_"));
        let dot_path = cache_dir.join(&filename);

        std::fs::write(&dot_path, dot_content)?;
        Ok(dot_path)
    }

    /// Export to DOT, convert to SVG, and open in default viewer
    pub fn export_and_open(graph: &NeighborhoodGraph, cache_dir: &Path) -> Result<PathBuf> {
        let dot_path = Self::export_to_file(graph, cache_dir)?;

        let svg_path = dot_path.with_extension("svg");

        // Convert DOT to SVG using graphviz
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

        // Open SVG in default viewer
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

    #[test]
    fn generate_dot_includes_header() {
        let graph = create_test_graph();

        let dot = DotExporter::generate_dot(&graph);

        assert!(dot.contains("digraph neighborhood {"));
        assert!(dot.contains("rankdir=LR"));
    }

    #[test]
    fn generate_dot_includes_center_node() {
        let graph = create_test_graph();

        let dot = DotExporter::generate_dot(&graph);

        assert!(dot.contains("\"public.users\""));
        assert!(dot.contains("fillcolor=gold"));
    }

    #[test]
    fn generate_dot_includes_related_nodes() {
        let graph = create_test_graph();

        let dot = DotExporter::generate_dot(&graph);

        assert!(dot.contains("\"public.orders\""));
        assert!(dot.contains("fillcolor=lightblue"));
    }

    #[test]
    fn generate_dot_includes_edges() {
        let graph = create_test_graph();

        let dot = DotExporter::generate_dot(&graph);

        assert!(dot.contains("\"public.orders\" -> \"public.users\""));
        assert!(dot.contains("label=\"fk_orders_user\""));
    }

    #[test]
    fn export_to_file_creates_dot_file() {
        let graph = create_test_graph();
        let temp_dir = tempfile::tempdir().unwrap();

        let result = DotExporter::export_to_file(&graph, temp_dir.path());

        assert!(result.is_ok());
        let dot_path = result.unwrap();
        assert!(dot_path.exists());
        assert!(dot_path.extension().map_or(false, |ext| ext == "dot"));
    }
}
