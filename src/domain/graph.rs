//! Neighborhood graph models for ER diagram visualization.
//!
//! Represents a subgraph of table relationships centered on a specific table,
//! showing FK connections within 1-2 hops.

/// A node in the neighborhood graph representing a database table.
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

    /// Returns the fully qualified table name (schema.table)
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

/// An edge in the neighborhood graph representing an FK relationship.
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

/// A neighborhood graph centered on a specific table.
///
/// Contains all tables within max_depth hops via FK relationships.
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

    /// Get outgoing edges from a node (this table references other tables)
    pub fn outgoing_edges(&self, qualified_name: &str) -> Vec<&GraphEdge> {
        self.edges
            .iter()
            .filter(|e| e.from_node == qualified_name)
            .collect()
    }

    /// Get incoming edges to a node (other tables reference this table)
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

    #[test]
    fn graph_node_qualified_name() {
        let node = GraphNode::new("public".to_string(), "users".to_string(), 0);
        assert_eq!(node.qualified_name(), "public.users");
    }

    #[test]
    fn graph_node_is_center() {
        let center = GraphNode::new("public".to_string(), "users".to_string(), 0);
        let neighbor = GraphNode::new("public".to_string(), "orders".to_string(), 1);

        assert!(center.is_center());
        assert!(!neighbor.is_center());
    }

    #[test]
    fn graph_edge_dedup_key_is_normalized() {
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
    fn graph_edge_direction() {
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
        assert_eq!(edge.direction_from("public.users"), EdgeDirection::Incoming);
    }

    #[test]
    fn neighborhood_graph_get_node() {
        let mut graph = NeighborhoodGraph::new("public.users".to_string(), 1);
        graph
            .nodes
            .push(GraphNode::new("public".to_string(), "users".to_string(), 0));
        graph.nodes.push(GraphNode::new(
            "public".to_string(),
            "orders".to_string(),
            1,
        ));

        assert!(graph.get_node("public.users").is_some());
        assert!(graph.get_node("public.orders").is_some());
        assert!(graph.get_node("public.products").is_none());
    }

    #[test]
    fn neighborhood_graph_edges_for_node() {
        let mut graph = NeighborhoodGraph::new("public.users".to_string(), 1);
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

        let user_edges = graph.edges_for_node("public.users");
        assert_eq!(user_edges.len(), 1);

        let order_edges = graph.edges_for_node("public.orders");
        assert_eq!(order_edges.len(), 2);
    }

    #[test]
    fn neighborhood_graph_outgoing_incoming_edges() {
        let mut graph = NeighborhoodGraph::new("public.orders".to_string(), 1);
        graph.edges.push(GraphEdge::new(
            "public.orders".to_string(),
            "public.users".to_string(),
            "fk_user".to_string(),
            vec!["user_id".to_string()],
            vec!["id".to_string()],
        ));

        let outgoing = graph.outgoing_edges("public.orders");
        let incoming = graph.incoming_edges("public.orders");

        assert_eq!(outgoing.len(), 1);
        assert_eq!(incoming.len(), 0);

        let user_incoming = graph.incoming_edges("public.users");
        assert_eq!(user_incoming.len(), 1);
    }
}
