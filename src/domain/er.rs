use std::collections::{HashMap, HashSet, VecDeque};

use crate::domain::Table;

#[derive(Debug, Clone)]
pub struct ErFkInfo {
    pub name: String,
    pub from_qualified: String,
    pub to_qualified: String,
}

#[derive(Debug, Clone)]
pub struct ErTableInfo {
    pub qualified_name: String,
    pub name: String,
    pub schema: String,
    pub foreign_keys: Vec<ErFkInfo>,
}

/// BFS on bidirectional FK adjacency graph from seed.
/// Returns the subset of tables reachable from the seed table via FK relationships.
/// Returns empty vec if seed is not found.
pub fn fk_reachable_tables(tables: &[ErTableInfo], seed: &str) -> Vec<ErTableInfo> {
    if !tables.iter().any(|t| t.qualified_name == seed) {
        return vec![];
    }

    // Build bidirectional adjacency map
    let mut adjacency: HashMap<&str, HashSet<&str>> = HashMap::new();
    for table in tables {
        for fk in &table.foreign_keys {
            adjacency
                .entry(fk.from_qualified.as_str())
                .or_default()
                .insert(fk.to_qualified.as_str());
            adjacency
                .entry(fk.to_qualified.as_str())
                .or_default()
                .insert(fk.from_qualified.as_str());
        }
    }

    // BFS from seed
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(seed);
    queue.push_back(seed);

    while let Some(current) = queue.pop_front() {
        if let Some(neighbors) = adjacency.get(current) {
            for &neighbor in neighbors {
                if visited.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }

    tables
        .iter()
        .filter(|t| visited.contains(t.qualified_name.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
fn make_table(name: &str, schema: &str, fks: Vec<(&str, &str)>) -> ErTableInfo {
    ErTableInfo {
        qualified_name: format!("{}.{}", schema, name),
        name: name.to_string(),
        schema: schema.to_string(),
        foreign_keys: fks
            .into_iter()
            .enumerate()
            .map(|(i, (from, to))| ErFkInfo {
                name: format!("fk_{}", i),
                from_qualified: from.to_string(),
                to_qualified: to.to_string(),
            })
            .collect(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    mod fk_reachable {
        use super::*;

        #[test]
        fn nonexistent_seed_returns_empty() {
            let tables = vec![make_table("users", "public", vec![])];

            let result = fk_reachable_tables(&tables, "public.missing");

            assert!(result.is_empty());
        }

        #[test]
        fn seed_only_no_fks_returns_seed() {
            let tables = vec![
                make_table("users", "public", vec![]),
                make_table("posts", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users");

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].qualified_name, "public.users");
        }

        #[test]
        fn direct_fk_returns_both() {
            let tables = vec![
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users");

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn transitive_fk_returns_chain() {
            // A -> B -> C
            let tables = vec![
                make_table(
                    "comments",
                    "public",
                    vec![("public.comments", "public.posts")],
                ),
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users");

            assert_eq!(result.len(), 3);
        }

        #[test]
        fn cyclic_fk_does_not_loop() {
            // A -> B -> A (cycle)
            let tables = vec![
                make_table("a", "public", vec![("public.a", "public.b")]),
                make_table("b", "public", vec![("public.b", "public.a")]),
            ];

            let result = fk_reachable_tables(&tables, "public.a");

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn disconnected_table_excluded() {
            let tables = vec![
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
                make_table("logs", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users");

            assert_eq!(result.len(), 2);
            assert!(!result.iter().any(|t| t.qualified_name == "public.logs"));
        }

        #[test]
        fn bidirectional_traversal() {
            // seed=posts, posts->users FK. Traversal should find users via reverse edge.
            let tables = vec![
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.posts");

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn cross_schema_fk() {
            let tables = vec![
                make_table("users", "public", vec![("public.users", "audit.logs")]),
                make_table("logs", "audit", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users");

            assert_eq!(result.len(), 2);
        }
    }
}
