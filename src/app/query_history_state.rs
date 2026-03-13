use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use super::input_mode::InputMode;
use super::text_input::TextInputState;
use crate::domain::query_history::QueryHistoryEntry;

#[derive(Debug, Clone, Default)]
pub struct QueryHistoryPickerState {
    pub entries: Vec<QueryHistoryEntry>,
    pub filter_input: TextInputState,
    pub selected: usize,
    pub scroll_offset: usize,
    pub pane_height: u16,
    pub origin_mode: Option<InputMode>,
}

pub struct FilteredEntry<'a> {
    pub entry: &'a QueryHistoryEntry,
    pub match_indices: Vec<u32>,
}

impl QueryHistoryPickerState {
    pub fn reset(&mut self) {
        self.entries.clear();
        self.filter_input.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.origin_mode = None;
    }

    pub fn filtered_entries(&self) -> Vec<FilteredEntry<'_>> {
        let filter = self.filter_input.content();

        // Return all entries in reverse order (newest first) when no filter
        if filter.is_empty() {
            return self
                .entries
                .iter()
                .rev()
                .map(|entry| FilteredEntry {
                    entry,
                    match_indices: Vec::new(),
                })
                .collect();
        }

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(filter, CaseMatching::Ignore, Normalization::Smart);

        self.entries
            .iter()
            .rev()
            .filter_map(|entry| {
                let mut indices = Vec::new();
                let mut buf = Vec::new();
                let haystack = nucleo_matcher::Utf32Str::new(&entry.query, &mut buf);
                let score = pattern.indices(haystack, &mut matcher, &mut indices);
                score.map(|_| FilteredEntry {
                    entry,
                    match_indices: indices,
                })
            })
            .collect()
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered_entries().len()
    }

    pub fn clamped_selected(&self) -> usize {
        let count = self.filtered_count();
        if count == 0 {
            0
        } else {
            self.selected.min(count.saturating_sub(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ConnectionId;

    fn make_entry(query: &str) -> QueryHistoryEntry {
        QueryHistoryEntry::new(
            query.to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
        )
    }

    fn make_state(entries: Vec<QueryHistoryEntry>) -> QueryHistoryPickerState {
        QueryHistoryPickerState {
            entries,
            ..Default::default()
        }
    }

    #[test]
    fn empty_filter_returns_all_entries_in_reverse_order() {
        let state = make_state(vec![
            make_entry("SELECT 1"),
            make_entry("SELECT 2"),
            make_entry("SELECT 3"),
        ]);

        let filtered = state.filtered_entries();

        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].entry.query, "SELECT 3");
        assert_eq!(filtered[1].entry.query, "SELECT 2");
        assert_eq!(filtered[2].entry.query, "SELECT 1");
    }

    #[test]
    fn fuzzy_filter_matches_partial_query() {
        let mut state = make_state(vec![
            make_entry("SELECT * FROM users"),
            make_entry("INSERT INTO orders VALUES (1)"),
            make_entry("SELECT count(*) FROM users"),
        ]);
        state.filter_input.set_content("users".to_string());

        let filtered = state.filtered_entries();

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|f| f.entry.query.contains("users")));
    }

    #[test]
    fn filter_is_case_insensitive() {
        let mut state = make_state(vec![make_entry("SELECT * FROM Users")]);
        state.filter_input.set_content("users".to_string());

        let filtered = state.filtered_entries();

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn reset_clears_all_state() {
        let mut state = QueryHistoryPickerState {
            entries: vec![make_entry("SELECT 1")],
            selected: 5,
            scroll_offset: 3,
            origin_mode: Some(InputMode::Normal),
            ..Default::default()
        };
        state.filter_input.set_content("test".to_string());

        state.reset();

        assert!(state.entries.is_empty());
        assert_eq!(state.filter_input.content(), "");
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll_offset, 0);
        assert!(state.origin_mode.is_none());
    }

    #[test]
    fn clamped_selected_with_empty_entries() {
        let state = QueryHistoryPickerState {
            selected: 5,
            ..Default::default()
        };

        assert_eq!(state.clamped_selected(), 0);
    }

    #[test]
    fn clamped_selected_clamps_to_last_index() {
        let state = QueryHistoryPickerState {
            entries: vec![make_entry("SELECT 1"), make_entry("SELECT 2")],
            selected: 10,
            ..Default::default()
        };

        assert_eq!(state.clamped_selected(), 1);
    }

    #[test]
    fn clamped_selected_preserves_valid_selection() {
        let state = QueryHistoryPickerState {
            entries: vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 2"),
                make_entry("SELECT 3"),
            ],
            selected: 1,
            ..Default::default()
        };

        assert_eq!(state.clamped_selected(), 1);
    }

    #[test]
    fn no_matches_returns_empty() {
        let mut state = make_state(vec![make_entry("SELECT 1")]);
        state.filter_input.set_content("xyz_no_match".to_string());

        let filtered = state.filtered_entries();

        assert!(filtered.is_empty());
    }
}
