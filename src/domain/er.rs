use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

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

/// Deterministic filename for ER diagram output.
pub fn er_output_filename(selected: &[String], total: usize) -> String {
    if selected.is_empty() || selected.len() == total {
        "er_full.dot".to_string()
    } else if selected.len() == 1 {
        let safe: String = selected[0]
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("er_partial_{}.dot", safe)
    } else {
        let mut sorted: Vec<&String> = selected.iter().collect();
        sorted.sort();
        let mut hasher = DefaultHasher::new();
        sorted.hash(&mut hasher);
        let hash = format!("{:016x}", hasher.finish());
        format!("er_partial_multi_{}_{}.dot", selected.len(), &hash[..8])
    }
}

/// Union of per-seed BFS results. Empty seeds → empty vec.
pub fn fk_reachable_tables_multi(
    tables: &[ErTableInfo],
    seeds: &[String],
    max_depth: usize,
) -> Vec<ErTableInfo> {
    let mut all_visited = HashSet::new();
    for seed in seeds {
        let reachable = fk_reachable_tables(tables, seed, max_depth);
        for t in &reachable {
            all_visited.insert(t.qualified_name.clone());
        }
    }
    tables
        .iter()
        .filter(|t| all_visited.contains(&t.qualified_name))
        .cloned()
        .collect()
}

/// BFS from seed on bidirectional FK graph with depth limit.
pub fn fk_reachable_tables(
    tables: &[ErTableInfo],
    seed: &str,
    max_depth: usize,
) -> Vec<ErTableInfo> {
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

    // BFS from seed with depth limit
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(seed);
    queue.push_back((seed, 0usize));

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(neighbors) = adjacency.get(current) {
            for &neighbor in neighbors {
                if visited.insert(neighbor) {
                    queue.push_back((neighbor, depth + 1));
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

        const UNLIMITED: usize = usize::MAX;

        #[test]
        fn nonexistent_seed_returns_empty() {
            let tables = vec![make_table("users", "public", vec![])];

            let result = fk_reachable_tables(&tables, "public.missing", UNLIMITED);

            assert!(result.is_empty());
        }

        #[test]
        fn seed_only_no_fks_returns_seed() {
            let tables = vec![
                make_table("users", "public", vec![]),
                make_table("posts", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users", UNLIMITED);

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].qualified_name, "public.users");
        }

        #[test]
        fn direct_fk_returns_both() {
            let tables = vec![
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users", UNLIMITED);

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

            let result = fk_reachable_tables(&tables, "public.users", UNLIMITED);

            assert_eq!(result.len(), 3);
        }

        #[test]
        fn cyclic_fk_does_not_loop() {
            // A -> B -> A (cycle)
            let tables = vec![
                make_table("a", "public", vec![("public.a", "public.b")]),
                make_table("b", "public", vec![("public.b", "public.a")]),
            ];

            let result = fk_reachable_tables(&tables, "public.a", UNLIMITED);

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn disconnected_table_excluded() {
            let tables = vec![
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
                make_table("logs", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users", UNLIMITED);

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

            let result = fk_reachable_tables(&tables, "public.posts", UNLIMITED);

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn cross_schema_fk() {
            let tables = vec![
                make_table("users", "public", vec![("public.users", "audit.logs")]),
                make_table("logs", "audit", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users", UNLIMITED);

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn depth_1_returns_direct_neighbors_only() {
            // A -> B -> C, depth=1 from A should return A and B only
            let tables = vec![
                make_table(
                    "comments",
                    "public",
                    vec![("public.comments", "public.posts")],
                ),
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users", 1);

            assert_eq!(result.len(), 2);
            assert!(result.iter().any(|t| t.qualified_name == "public.users"));
            assert!(result.iter().any(|t| t.qualified_name == "public.posts"));
            assert!(!result.iter().any(|t| t.qualified_name == "public.comments"));
        }

        #[test]
        fn depth_2_returns_two_hops() {
            // A -> B -> C, depth=2 from A should return all three
            let tables = vec![
                make_table(
                    "comments",
                    "public",
                    vec![("public.comments", "public.posts")],
                ),
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
            ];

            let result = fk_reachable_tables(&tables, "public.users", 2);

            assert_eq!(result.len(), 3);
        }
    }

    mod fk_reachable_multi {
        use super::*;

        #[test]
        fn empty_seeds_returns_empty() {
            let tables = vec![make_table("users", "public", vec![])];

            let result = fk_reachable_tables_multi(&tables, &[], 1);

            assert!(result.is_empty());
        }

        #[test]
        fn single_seed_matches_single_fn() {
            let tables = vec![
                make_table("posts", "public", vec![("public.posts", "public.users")]),
                make_table("users", "public", vec![]),
                make_table("logs", "public", vec![]),
            ];

            let result = fk_reachable_tables_multi(&tables, &["public.users".to_string()], 1);

            assert_eq!(result.len(), 2);
            assert!(!result.iter().any(|t| t.qualified_name == "public.logs"));
        }

        #[test]
        fn multi_seeds_union() {
            let tables = vec![
                make_table("a", "public", vec![("public.a", "public.b")]),
                make_table("b", "public", vec![]),
                make_table("c", "public", vec![]),
            ];
            let seeds = vec!["public.a".to_string(), "public.c".to_string()];

            let result = fk_reachable_tables_multi(&tables, &seeds, 1);

            assert_eq!(result.len(), 3);
        }

        #[test]
        fn overlap_dedup() {
            let tables = vec![
                make_table("a", "public", vec![("public.a", "public.b")]),
                make_table("b", "public", vec![]),
            ];
            let seeds = vec!["public.a".to_string(), "public.b".to_string()];

            let result = fk_reachable_tables_multi(&tables, &seeds, 1);

            assert_eq!(result.len(), 2);
        }

        #[test]
        fn invalid_seed_ignored() {
            let tables = vec![make_table("users", "public", vec![])];
            let seeds = vec!["public.users".to_string(), "public.missing".to_string()];

            let result = fk_reachable_tables_multi(&tables, &seeds, 1);

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].qualified_name, "public.users");
        }
    }
}
