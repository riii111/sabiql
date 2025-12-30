//! Graph builder for constructing neighborhood graphs from FK relationships.
//!
//! Uses BFS traversal to find all tables within N hops of a center table.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::domain::{GraphEdge, GraphNode, NeighborhoodGraph, Table};

pub struct GraphBuilder;

impl GraphBuilder {
    pub fn build<'a, I>(center_table: &str, table_details: I, max_depth: u8) -> NeighborhoodGraph
    where
        I: IntoIterator<Item = (&'a String, &'a Table)>,
    {
        // Pre-index tables for O(1) lookup
        let table_map: HashMap<&str, &Table> = table_details
            .into_iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        let mut graph = NeighborhoodGraph::new(center_table.to_string(), max_depth);
        let mut visited: HashSet<&str> = HashSet::new();
        let mut edge_keys: HashSet<(String, String, String)> = HashSet::new();
        let mut queue: VecDeque<(String, u8)> = VecDeque::new();

        queue.push_back((center_table.to_string(), 0));

        while let Some((current, depth)) = queue.pop_front() {
            if visited.contains(current.as_str()) || depth > max_depth {
                continue;
            }
            let current_key = table_map.keys().find(|&&k| k == current.as_str()).copied();
            if let Some(key) = current_key {
                visited.insert(key);
            } else {
                // Table not in cache - still add node but can't traverse FKs
                if let Some((schema, table)) = Self::split_qualified_name(&current) {
                    graph
                        .nodes
                        .push(GraphNode::new(schema.to_string(), table.to_string(), depth));
                }
                continue;
            }

            if let Some((schema, table)) = Self::split_qualified_name(&current) {
                graph
                    .nodes
                    .push(GraphNode::new(schema.to_string(), table.to_string(), depth));
            }

            // Find outgoing FKs (this table references other tables)
            if let Some(table_detail) = table_map.get(current.as_str()) {
                for fk in &table_detail.foreign_keys {
                    let target = fk.referenced_table();

                    let edge = GraphEdge::new(
                        current.clone(),
                        target.clone(),
                        fk.name.clone(),
                        fk.from_columns.clone(),
                        fk.to_columns.clone(),
                    );

                    let key = edge.dedup_key();
                    if !edge_keys.contains(&key) {
                        edge_keys.insert(key);
                        graph.edges.push(edge);
                    }

                    if !visited.contains(target.as_str()) && depth < max_depth {
                        queue.push_back((target, depth + 1));
                    }
                }
            }

            // Find incoming FKs (other tables reference this table)
            for (&qualified_name, table_detail) in &table_map {
                if visited.contains(qualified_name) {
                    continue;
                }

                for fk in &table_detail.foreign_keys {
                    if fk.referenced_table() == current {
                        let edge = GraphEdge::new(
                            qualified_name.to_string(),
                            current.clone(),
                            fk.name.clone(),
                            fk.from_columns.clone(),
                            fk.to_columns.clone(),
                        );

                        let key = edge.dedup_key();
                        if !edge_keys.contains(&key) {
                            edge_keys.insert(key);
                            graph.edges.push(edge);
                        }

                        if depth < max_depth {
                            queue.push_back((qualified_name.to_string(), depth + 1));
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

        // Sort edges for deterministic output
        graph.edges.sort_by(|a, b| {
            a.from_node
                .cmp(&b.from_node)
                .then_with(|| a.to_node.cmp(&b.to_node))
                .then_with(|| a.fk_name.cmp(&b.fk_name))
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

    mod build {
        use super::*;

        #[test]
        fn single_table_without_fks_returns_only_center_node() {
            let mut tables: HashMap<String, Table> = HashMap::new();
            tables.insert(
                "public.users".to_string(),
                make_table("public", "users", vec![]),
            );

            let graph = GraphBuilder::build("public.users", &tables, 1);

            assert_eq!(graph.nodes.len(), 1);
            assert!(graph.center_node().unwrap().is_center());
        }

        #[test]
        fn single_table_without_fks_returns_no_edges() {
            let mut tables: HashMap<String, Table> = HashMap::new();
            tables.insert(
                "public.users".to_string(),
                make_table("public", "users", vec![]),
            );

            let graph = GraphBuilder::build("public.users", &tables, 1);

            assert_eq!(graph.edges.len(), 0);
        }

        #[test]
        fn outgoing_fk_includes_referenced_table_as_neighbor() {
            let mut tables: HashMap<String, Table> = HashMap::new();
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
            assert_eq!(
                graph.center_node().unwrap().qualified_name(),
                "public.orders"
            );
        }

        #[test]
        fn outgoing_fk_creates_edge_from_source_to_target() {
            let mut tables: HashMap<String, Table> = HashMap::new();
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

            assert_eq!(graph.edges.len(), 1);
            assert_eq!(graph.edges[0].from_node, "public.orders");
            assert_eq!(graph.edges[0].to_node, "public.users");
        }

        #[test]
        fn incoming_fk_includes_referencing_table_as_neighbor() {
            let mut tables: HashMap<String, Table> = HashMap::new();
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

            let graph = GraphBuilder::build("public.users", &tables, 1);

            assert_eq!(graph.nodes.len(), 2);
            assert_eq!(
                graph.center_node().unwrap().qualified_name(),
                "public.users"
            );
        }

        #[test]
        fn depth_one_excludes_two_hop_neighbors() {
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

            let graph = GraphBuilder::build("public.users", &tables, 1);

            assert_eq!(graph.nodes.len(), 2);
        }

        #[test]
        fn depth_two_includes_two_hop_neighbors() {
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

            let graph = GraphBuilder::build("public.users", &tables, 2);

            assert_eq!(graph.nodes.len(), 3);
        }

        #[test]
        fn cyclic_references_visit_all_nodes_without_infinite_loop() {
            let mut tables: HashMap<String, Table> = HashMap::new();
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

            assert_eq!(graph.nodes.len(), 3);
        }

        #[test]
        fn cyclic_references_include_all_fk_edges() {
            let mut tables: HashMap<String, Table> = HashMap::new();
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

            assert_eq!(graph.edges.len(), 3);
        }

        #[test]
        fn multiple_fks_to_same_table_preserves_distinct_edges() {
            let mut tables: HashMap<String, Table> = HashMap::new();
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

            assert_eq!(graph.edges.len(), 2);
        }

        #[test]
        fn nodes_are_sorted_by_hop_distance_then_name() {
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

            assert_eq!(graph.nodes[0].qualified_name(), "public.users");
            assert_eq!(graph.nodes[1].qualified_name(), "public.addresses");
            assert_eq!(graph.nodes[2].qualified_name(), "public.orders");
        }
    }
}
