use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErStatus {
    #[default]
    Idle,
    Waiting,
    Rendering,
}

#[derive(Debug, Clone, Default)]
pub struct ErPreparationState {
    pub pending_tables: HashSet<String>,
    pub fetching_tables: HashSet<String>,
    pub failed_tables: HashMap<String, String>,
    pub started: bool,
    pub status: ErStatus,
}

impl ErPreparationState {
    pub fn is_complete(&self) -> bool {
        self.pending_tables.is_empty() && self.fetching_tables.is_empty()
    }

    pub fn has_failures(&self) -> bool {
        !self.failed_tables.is_empty()
    }

    pub fn on_table_cached(&mut self, qualified_name: &str) {
        self.fetching_tables.remove(qualified_name);
        self.pending_tables.remove(qualified_name);
    }

    pub fn on_table_failed(&mut self, qualified_name: &str, error: String) {
        self.fetching_tables.remove(qualified_name);
        self.failed_tables.insert(qualified_name.to_string(), error);
    }

    pub fn retry_failed(&mut self) {
        for (table, _) in self.failed_tables.drain() {
            self.pending_tables.insert(table);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod is_complete {
        use super::*;

        #[test]
        fn empty_state_returns_true() {
            let state = ErPreparationState::default();

            assert!(state.is_complete());
        }

        #[test]
        fn pending_tables_returns_false() {
            let mut state = ErPreparationState::default();
            state.pending_tables.insert("public.users".to_string());

            assert!(!state.is_complete());
        }

        #[test]
        fn fetching_tables_returns_false() {
            let mut state = ErPreparationState::default();
            state.fetching_tables.insert("public.users".to_string());

            assert!(!state.is_complete());
        }
    }

    mod on_table_cached {
        use super::*;

        #[test]
        fn removes_from_fetching() {
            let mut state = ErPreparationState::default();
            state.fetching_tables.insert("public.users".to_string());

            state.on_table_cached("public.users");

            assert!(!state.fetching_tables.contains("public.users"));
        }

        #[test]
        fn removes_from_pending() {
            let mut state = ErPreparationState::default();
            state.pending_tables.insert("public.users".to_string());

            state.on_table_cached("public.users");

            assert!(!state.pending_tables.contains("public.users"));
        }
    }

    mod on_table_failed {
        use super::*;

        #[test]
        fn moves_from_fetching_to_failed() {
            let mut state = ErPreparationState::default();
            state.fetching_tables.insert("public.users".to_string());

            state.on_table_failed("public.users", "timeout".to_string());

            assert!(!state.fetching_tables.contains("public.users"));
            assert!(state.failed_tables.contains_key("public.users"));
        }
    }

    mod retry_failed {
        use super::*;

        #[test]
        fn moves_failed_to_pending() {
            let mut state = ErPreparationState::default();
            state
                .failed_tables
                .insert("public.users".to_string(), "error".to_string());

            state.retry_failed();

            assert!(state.failed_tables.is_empty());
            assert!(state.pending_tables.contains("public.users"));
        }
    }

    mod reset {
        use super::*;

        #[test]
        fn clears_all_state() {
            let mut state = ErPreparationState {
                pending_tables: HashSet::from(["a".to_string()]),
                fetching_tables: HashSet::from(["b".to_string()]),
                failed_tables: HashMap::from([("c".to_string(), "err".to_string())]),
                started: true,
                status: ErStatus::Waiting,
            };

            state.reset();

            assert!(state.pending_tables.is_empty());
            assert!(state.fetching_tables.is_empty());
            assert!(state.failed_tables.is_empty());
            assert!(!state.started);
            assert_eq!(state.status, ErStatus::Idle);
        }
    }
}
