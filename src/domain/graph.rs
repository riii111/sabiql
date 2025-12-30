//! Neighborhood graph models for ER diagram visualization.
//!
//! Represents a subgraph of table relationships centered on a specific table,
//! showing FK connections within 1-2 hops.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub schema: String,
    pub table: String,
    /// Distance from the center table (0 = center, 1 = direct neighbor, 2 = 2-hop)
    pub hop_distance: u8,
}

impl GraphNode {
    pub fn new(schema: String, table: String, hop_distance: u8) -> Self {
        Self {
            schema,
            table,
            hop_distance,
        }
    }

    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.schema, self.table)
    }

    pub fn is_center(&self) -> bool {
        self.hop_distance == 0
    }
}

/// Direction of the FK relationship relative to the source node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDirection {
    /// This table has an FK pointing to another table (outgoing reference)
    Outgoing,
    /// Another table has an FK pointing to this table (incoming reference)
    Incoming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    /// Source table (schema.table) - the table that has the FK constraint
    pub from_node: String,
    /// Target table (schema.table) - the table being referenced
    pub to_node: String,
    pub fk_name: String,
    pub from_columns: Vec<String>,
    pub to_columns: Vec<String>,
}

impl GraphEdge {
    pub fn new(
        from_node: String,
        to_node: String,
        fk_name: String,
        from_columns: Vec<String>,
        to_columns: Vec<String>,
    ) -> Self {
        Self {
            from_node,
            to_node,
            fk_name,
            from_columns,
            to_columns,
        }
    }

    /// Returns a normalized key for deduplication (alphabetically sorted endpoints + fk_name)
    pub fn dedup_key(&self) -> (String, String, String) {
        if self.from_node <= self.to_node {
            (
                self.from_node.clone(),
                self.to_node.clone(),
                self.fk_name.clone(),
            )
        } else {
            (
                self.to_node.clone(),
                self.from_node.clone(),
                self.fk_name.clone(),
            )
        }
    }

    pub fn direction_from(&self, table: &str) -> EdgeDirection {
        if self.from_node == table {
            EdgeDirection::Outgoing
        } else {
            EdgeDirection::Incoming
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NeighborhoodGraph {
    pub center: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub max_depth: u8,
}

impl NeighborhoodGraph {
    pub fn new(center: String, max_depth: u8) -> Self {
        Self {
            center,
            nodes: Vec::new(),
            edges: Vec::new(),
            max_depth,
        }
    }

    pub fn get_node(&self, qualified_name: &str) -> Option<&GraphNode> {
        self.nodes
            .iter()
            .find(|n| n.qualified_name() == qualified_name)
    }

    pub fn center_node(&self) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.is_center())
    }

    pub fn edges_for_node(&self, qualified_name: &str) -> Vec<&GraphEdge> {
        self.edges
            .iter()
            .filter(|e| e.from_node == qualified_name || e.to_node == qualified_name)
            .collect()
    }

    pub fn outgoing_edges(&self, qualified_name: &str) -> Vec<&GraphEdge> {
        self.edges
            .iter()
            .filter(|e| e.from_node == qualified_name)
            .collect()
    }

    pub fn incoming_edges(&self, qualified_name: &str) -> Vec<&GraphEdge> {
        self.edges
            .iter()
            .filter(|e| e.to_node == qualified_name)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod graph_node {
        use super::*;

        #[test]
        fn qualified_name_returns_schema_dot_table() {
            let node = GraphNode::new("public".to_string(), "users".to_string(), 0);

            assert_eq!(node.qualified_name(), "public.users");
        }

        #[test]
        fn is_center_returns_true_when_hop_distance_zero() {
            let node = GraphNode::new("public".to_string(), "users".to_string(), 0);

            assert!(node.is_center());
        }

        #[test]
        fn is_center_returns_false_when_hop_distance_nonzero() {
            let node = GraphNode::new("public".to_string(), "orders".to_string(), 1);

            assert!(!node.is_center());
        }
    }

    mod graph_edge {
        use super::*;

        #[test]
        fn dedup_key_normalizes_endpoint_order() {
            let edge1 = GraphEdge::new(
                "public.users".to_string(),
                "public.orders".to_string(),
                "fk_user".to_string(),
                vec!["user_id".to_string()],
                vec!["id".to_string()],
            );
            let edge2 = GraphEdge::new(
                "public.orders".to_string(),
                "public.users".to_string(),
                "fk_user".to_string(),
                vec!["id".to_string()],
                vec!["user_id".to_string()],
            );

            assert_eq!(edge1.dedup_key(), edge2.dedup_key());
        }

        #[test]
        fn direction_from_returns_outgoing_for_from_node() {
            let edge = GraphEdge::new(
                "public.orders".to_string(),
                "public.users".to_string(),
                "fk_user".to_string(),
                vec!["user_id".to_string()],
                vec!["id".to_string()],
            );

            assert_eq!(
                edge.direction_from("public.orders"),
                EdgeDirection::Outgoing
            );
        }

        #[test]
        fn direction_from_returns_incoming_for_to_node() {
            let edge = GraphEdge::new(
                "public.orders".to_string(),
                "public.users".to_string(),
                "fk_user".to_string(),
                vec!["user_id".to_string()],
                vec!["id".to_string()],
            );

            assert_eq!(edge.direction_from("public.users"), EdgeDirection::Incoming);
        }
    }

    mod neighborhood_graph {
        use super::*;

        #[test]
        fn get_node_returns_some_when_exists() {
            let mut graph = NeighborhoodGraph::new("public.users".to_string(), 1);
            graph
                .nodes
                .push(GraphNode::new("public".to_string(), "users".to_string(), 0));

            assert!(graph.get_node("public.users").is_some());
        }

        #[test]
        fn get_node_returns_none_when_not_found() {
            let graph = NeighborhoodGraph::new("public.users".to_string(), 1);

            assert!(graph.get_node("public.products").is_none());
        }

        #[test]
        fn edges_for_node_returns_all_connected_edges() {
            let mut graph = NeighborhoodGraph::new("public.orders".to_string(), 1);
            graph.edges.push(GraphEdge::new(
                "public.orders".to_string(),
                "public.users".to_string(),
                "fk_user".to_string(),
                vec!["user_id".to_string()],
                vec!["id".to_string()],
            ));
            graph.edges.push(GraphEdge::new(
                "public.orders".to_string(),
                "public.products".to_string(),
                "fk_product".to_string(),
                vec!["product_id".to_string()],
                vec!["id".to_string()],
            ));

            assert_eq!(graph.edges_for_node("public.orders").len(), 2);
        }

        #[test]
        fn outgoing_edges_returns_edges_from_node() {
            let mut graph = NeighborhoodGraph::new("public.orders".to_string(), 1);
            graph.edges.push(GraphEdge::new(
                "public.orders".to_string(),
                "public.users".to_string(),
                "fk_user".to_string(),
                vec!["user_id".to_string()],
                vec!["id".to_string()],
            ));

            assert_eq!(graph.outgoing_edges("public.orders").len(), 1);
        }

        #[test]
        fn incoming_edges_returns_edges_to_node() {
            let mut graph = NeighborhoodGraph::new("public.orders".to_string(), 1);
            graph.edges.push(GraphEdge::new(
                "public.orders".to_string(),
                "public.users".to_string(),
                "fk_user".to_string(),
                vec!["user_id".to_string()],
                vec!["id".to_string()],
            ));

            assert_eq!(graph.incoming_edges("public.users").len(), 1);
        }
    }
}
