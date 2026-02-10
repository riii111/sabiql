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
