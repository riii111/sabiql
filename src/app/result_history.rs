use std::collections::VecDeque;

use crate::domain::QueryResult;

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

impl ResultHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

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
    fn push_and_get_returns_entries_in_order() {
        let mut history = ResultHistory::new(3);

        history.push(make_result("SELECT 1"));
        history.push(make_result("SELECT 2"));

        assert_eq!(history.get(0).unwrap().query, "SELECT 1");
        assert_eq!(history.get(1).unwrap().query, "SELECT 2");
        assert!(history.get(2).is_none());
    }

    #[test]
    fn push_evicts_oldest_when_at_capacity() {
        let mut history = ResultHistory::new(2);

        history.push(make_result("SELECT 1"));
        history.push(make_result("SELECT 2"));
        history.push(make_result("SELECT 3"));

        // SELECT 1 should be evicted
        assert_eq!(history.get(0).unwrap().query, "SELECT 2");
        assert_eq!(history.get(1).unwrap().query, "SELECT 3");
        assert!(history.get(2).is_none());
    }
}
