//! Graph builder for constructing neighborhood graphs from FK relationships.
//!
//! Uses BFS traversal to find all tables within N hops of a center table.

use std::collections::{HashSet, VecDeque};

use crate::domain::{GraphEdge, GraphNode, NeighborhoodGraph, Table};

/// Builds neighborhood graphs from table FK metadata.
pub struct GraphBuilder;

impl GraphBuilder {
    /// Build a neighborhood graph centered on the given table.
    ///
    /// # Arguments
    /// * `center_table` - Qualified name of center table (schema.table)
    /// * `table_details` - Map of qualified table names to their details
    /// * `max_depth` - Maximum hop distance (1 or 2)
    ///
    /// # Returns
    /// A NeighborhoodGraph containing all tables within max_depth hops
    pub fn build<'a, I>(center_table: &str, table_details: I, max_depth: u8) -> NeighborhoodGraph
    where
        I: IntoIterator<Item = (&'a String, &'a Table)> + Clone,
    {
        let mut graph = NeighborhoodGraph::new(center_table.to_string(), max_depth);
        let mut visited: HashSet<String> = HashSet::new();
        let mut edge_keys: HashSet<(String, String, String)> = HashSet::new();
        let mut queue: VecDeque<(String, u8)> = VecDeque::new();

        queue.push_back((center_table.to_string(), 0));

        while let Some((current, depth)) = queue.pop_front() {
            if visited.contains(&current) || depth > max_depth {
                continue;
            }
            visited.insert(current.clone());

            // Add node
            if let Some((schema, table)) = Self::split_qualified_name(&current) {
                graph
                    .nodes
                    .push(GraphNode::new(schema.to_string(), table.to_string(), depth));
            }

            // Find outgoing FKs (this table references other tables)
            for (qualified_name, table_detail) in table_details.clone() {
                if *qualified_name == current {
                    for fk in &table_detail.foreign_keys {
                        let target = fk.referenced_table();

                        let edge = GraphEdge::new(
                            current.clone(),
                            target.clone(),
                            fk.name.clone(),
                            fk.from_columns.clone(),
                            fk.to_columns.clone(),
                        );

                        // Deduplicate edges
                        let key = edge.dedup_key();
                        if !edge_keys.contains(&key) {
                            edge_keys.insert(key);
                            graph.edges.push(edge);
                        }

                        // Queue target for traversal
                        if !visited.contains(&target) && depth < max_depth {
                            queue.push_back((target, depth + 1));
                        }
                    }
                }
            }

            // Find incoming FKs (other tables reference this table)
            for (qualified_name, table_detail) in table_details.clone() {
                if visited.contains(qualified_name) {
                    continue;
                }

                for fk in &table_detail.foreign_keys {
                    if fk.referenced_table() == current {
                        let edge = GraphEdge::new(
                            qualified_name.clone(),
                            current.clone(),
                            fk.name.clone(),
                            fk.from_columns.clone(),
                            fk.to_columns.clone(),
                        );

                        // Deduplicate edges
                        let key = edge.dedup_key();
                        if !edge_keys.contains(&key) {
                            edge_keys.insert(key);
                            graph.edges.push(edge);
                        }

                        // Queue source table for traversal
                        if depth < max_depth {
                            queue.push_back((qualified_name.clone(), depth + 1));
                        }
                    }
                }
            }
        }

        // Sort nodes by hop distance, then by name for consistent display
        graph.nodes.sort_by(|a, b| {
            a.hop_distance
                .cmp(&b.hop_distance)
                .then_with(|| a.qualified_name().cmp(&b.qualified_name()))
        });

        graph
    }

    fn split_qualified_name(qualified: &str) -> Option<(&str, &str)> {
        qualified.split_once('.')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, FkAction, ForeignKey};
    use std::collections::HashMap;

    fn make_table(schema: &str, name: &str, fks: Vec<ForeignKey>) -> Table {
        Table {
            schema: schema.to_string(),
            name: name.to_string(),
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
            foreign_keys: fks,
            indexes: vec![],
            rls: None,
            row_count_estimate: Some(100),
            comment: None,
        }
    }

    fn make_fk(
        name: &str,
        from_schema: &str,
        from_table: &str,
        from_col: &str,
        to_schema: &str,
        to_table: &str,
        to_col: &str,
    ) -> ForeignKey {
        ForeignKey {
            name: name.to_string(),
            from_schema: from_schema.to_string(),
            from_table: from_table.to_string(),
            from_columns: vec![from_col.to_string()],
            to_schema: to_schema.to_string(),
            to_table: to_table.to_string(),
            to_columns: vec![to_col.to_string()],
            on_delete: FkAction::NoAction,
            on_update: FkAction::NoAction,
        }
    }

    #[test]
    fn build_graph_single_table_no_fks() {
        let mut tables: HashMap<String, Table> = HashMap::new();
        tables.insert(
            "public.users".to_string(),
            make_table("public", "users", vec![]),
        );

        let graph = GraphBuilder::build("public.users", &tables, 1);

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.edges.len(), 0);
        assert!(graph.center_node().unwrap().is_center());
    }

    #[test]
    fn build_graph_with_outgoing_fk() {
        let mut tables: HashMap<String, Table> = HashMap::new();

        // orders references users (orders.user_id -> users.id)
        tables.insert(
            "public.orders".to_string(),
            make_table(
                "public",
                "orders",
                vec![make_fk(
                    "fk_user", "public", "orders", "user_id", "public", "users", "id",
                )],
            ),
        );
        tables.insert(
            "public.users".to_string(),
            make_table("public", "users", vec![]),
        );

        let graph = GraphBuilder::build("public.orders", &tables, 1);

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);

        let center = graph.center_node().unwrap();
        assert_eq!(center.qualified_name(), "public.orders");

        let edge = &graph.edges[0];
        assert_eq!(edge.from_node, "public.orders");
        assert_eq!(edge.to_node, "public.users");
    }

    #[test]
    fn build_graph_with_incoming_fk() {
        let mut tables: HashMap<String, Table> = HashMap::new();

        // orders references users (orders.user_id -> users.id)
        tables.insert(
            "public.orders".to_string(),
            make_table(
                "public",
                "orders",
                vec![make_fk(
                    "fk_user", "public", "orders", "user_id", "public", "users", "id",
                )],
            ),
        );
        tables.insert(
            "public.users".to_string(),
            make_table("public", "users", vec![]),
        );

        // Center on users - should find orders as incoming
        let graph = GraphBuilder::build("public.users", &tables, 1);

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);

        let center = graph.center_node().unwrap();
        assert_eq!(center.qualified_name(), "public.users");
    }

    #[test]
    fn build_graph_respects_max_depth() {
        let mut tables: HashMap<String, Table> = HashMap::new();

        // Chain: users -> orders -> order_items
        tables.insert(
            "public.users".to_string(),
            make_table("public", "users", vec![]),
        );
        tables.insert(
            "public.orders".to_string(),
            make_table(
                "public",
                "orders",
                vec![make_fk(
                    "fk_user", "public", "orders", "user_id", "public", "users", "id",
                )],
            ),
        );
        tables.insert(
            "public.order_items".to_string(),
            make_table(
                "public",
                "order_items",
                vec![make_fk(
                    "fk_order",
                    "public",
                    "order_items",
                    "order_id",
                    "public",
                    "orders",
                    "id",
                )],
            ),
        );

        // Depth 1: users + orders
        let graph1 = GraphBuilder::build("public.users", &tables, 1);
        assert_eq!(graph1.nodes.len(), 2);

        // Depth 2: users + orders + order_items
        let graph2 = GraphBuilder::build("public.users", &tables, 2);
        assert_eq!(graph2.nodes.len(), 3);
    }

    #[test]
    fn build_graph_handles_cycles() {
        let mut tables: HashMap<String, Table> = HashMap::new();

        // Circular: a -> b -> c -> a
        tables.insert(
            "public.a".to_string(),
            make_table(
                "public",
                "a",
                vec![make_fk("fk_b", "public", "a", "b_id", "public", "b", "id")],
            ),
        );
        tables.insert(
            "public.b".to_string(),
            make_table(
                "public",
                "b",
                vec![make_fk("fk_c", "public", "b", "c_id", "public", "c", "id")],
            ),
        );
        tables.insert(
            "public.c".to_string(),
            make_table(
                "public",
                "c",
                vec![make_fk("fk_a", "public", "c", "a_id", "public", "a", "id")],
            ),
        );

        let graph = GraphBuilder::build("public.a", &tables, 2);

        // Should visit all 3 nodes without infinite loop
        assert_eq!(graph.nodes.len(), 3);
        // Should have 3 edges (one for each FK)
        assert_eq!(graph.edges.len(), 3);
    }

    #[test]
    fn build_graph_deduplicates_edges() {
        let mut tables: HashMap<String, Table> = HashMap::new();

        // Two FKs from orders to users (unusual but possible)
        tables.insert(
            "public.orders".to_string(),
            make_table(
                "public",
                "orders",
                vec![
                    make_fk(
                        "fk_user", "public", "orders", "user_id", "public", "users", "id",
                    ),
                    make_fk(
                        "fk_created_by",
                        "public",
                        "orders",
                        "created_by",
                        "public",
                        "users",
                        "id",
                    ),
                ],
            ),
        );
        tables.insert(
            "public.users".to_string(),
            make_table("public", "users", vec![]),
        );

        let graph = GraphBuilder::build("public.orders", &tables, 1);

        // Both FKs should be present (different names = different edges)
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn nodes_sorted_by_hop_then_name() {
        let mut tables: HashMap<String, Table> = HashMap::new();

        tables.insert(
            "public.users".to_string(),
            make_table("public", "users", vec![]),
        );
        tables.insert(
            "public.orders".to_string(),
            make_table(
                "public",
                "orders",
                vec![make_fk(
                    "fk_user", "public", "orders", "user_id", "public", "users", "id",
                )],
            ),
        );
        tables.insert(
            "public.addresses".to_string(),
            make_table(
                "public",
                "addresses",
                vec![make_fk(
                    "fk_user",
                    "public",
                    "addresses",
                    "user_id",
                    "public",
                    "users",
                    "id",
                )],
            ),
        );

        let graph = GraphBuilder::build("public.users", &tables, 1);

        // First should be center (hop 0), then hop 1 sorted alphabetically
        assert_eq!(graph.nodes[0].qualified_name(), "public.users");
        assert_eq!(graph.nodes[1].qualified_name(), "public.addresses");
        assert_eq!(graph.nodes[2].qualified_name(), "public.orders");
    }
}
