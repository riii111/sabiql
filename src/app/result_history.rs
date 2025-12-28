use std::collections::VecDeque;

use crate::domain::QueryResult;

/// Ring buffer for storing query result history
#[derive(Debug, Clone)]
pub struct ResultHistory {
    entries: VecDeque<QueryResult>,
    capacity: usize,
}

impl Default for ResultHistory {
    fn default() -> Self {
        Self::new(20)
    }
}

#[allow(dead_code)]
impl ResultHistory {
    /// Create a new result history with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Add a query result to the history
    /// If the history is at capacity, the oldest entry is removed
    pub fn push(&mut self, result: QueryResult) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(result);
    }

    /// Get a result by index (0 = oldest, len-1 = newest)
    pub fn get(&self, index: usize) -> Option<&QueryResult> {
        self.entries.get(index)
    }

    /// Get the most recent result
    pub fn latest(&self) -> Option<&QueryResult> {
        self.entries.back()
    }

    /// Get the number of stored results
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get an iterator over the results (oldest first)
    pub fn iter(&self) -> impl Iterator<Item = &QueryResult> {
        self.entries.iter()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get the capacity of the history
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::QuerySource;

    fn make_result(query: &str) -> QueryResult {
        QueryResult::success(
            query.to_string(),
            vec!["col1".to_string()],
            vec![vec!["val1".to_string()]],
            10,
            QuerySource::Adhoc,
        )
    }

    #[test]
    fn test_push_and_get() {
        let mut history = ResultHistory::new(3);

        history.push(make_result("SELECT 1"));
        history.push(make_result("SELECT 2"));

        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().query, "SELECT 1");
        assert_eq!(history.get(1).unwrap().query, "SELECT 2");
    }

    #[test]
    fn test_capacity_limit() {
        let mut history = ResultHistory::new(2);

        history.push(make_result("SELECT 1"));
        history.push(make_result("SELECT 2"));
        history.push(make_result("SELECT 3"));

        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().query, "SELECT 2");
        assert_eq!(history.get(1).unwrap().query, "SELECT 3");
    }

    #[test]
    fn test_latest() {
        let mut history = ResultHistory::new(3);

        assert!(history.latest().is_none());

        history.push(make_result("SELECT 1"));
        history.push(make_result("SELECT 2"));

        assert_eq!(history.latest().unwrap().query, "SELECT 2");
    }
}
