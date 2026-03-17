use std::time::Instant;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use super::text_input::TextInputState;
use crate::domain::query_history::QueryHistoryEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryHistoryPickerMode {
    #[default]
    Normal,
    Filter,
}

#[derive(Debug, Clone, Default)]
pub struct QueryHistoryPickerState {
    pub mode: QueryHistoryPickerMode,
    pub entries: Vec<QueryHistoryEntry>,
    pub filter_input: TextInputState,
    pub selected: usize,
    pub scroll_offset: usize,
    pub pane_height: u16,
    pub yank_flash_until: Option<Instant>,
}

pub struct FilteredEntry<'a> {
    pub entry: &'a QueryHistoryEntry,
    pub match_indices: Vec<u32>,
}

pub struct GroupedEntry<'a> {
    pub entry: &'a QueryHistoryEntry,
    pub count: usize,
    pub match_indices: Vec<u32>,
}

impl QueryHistoryPickerState {
    pub fn reset(&mut self) {
        self.mode = QueryHistoryPickerMode::Normal;
        self.entries.clear();
        self.filter_input.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.yank_flash_until = None;
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

    pub fn grouped_filtered_entries(&self) -> Vec<GroupedEntry<'_>> {
        let filtered = self.filtered_entries();
        let mut groups: Vec<GroupedEntry<'_>> = Vec::new();

        for fe in filtered {
            if let Some(last) = groups
                .last_mut()
                .filter(|g| g.entry.query == fe.entry.query)
            {
                last.count += 1;
                continue;
            }
            groups.push(GroupedEntry {
                entry: fe.entry,
                count: 1,
                match_indices: fe.match_indices,
            });
        }

        groups
    }

    pub fn grouped_count(&self) -> usize {
        self.grouped_filtered_entries().len()
    }

    pub fn clamped_selected(&self) -> usize {
        let count = self.grouped_count();
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
        use crate::domain::query_history::QueryResultStatus;
        QueryHistoryEntry::new(
            query.to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            QueryResultStatus::Success,
            None,
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
            ..Default::default()
        };
        state.filter_input.set_content("test".to_string());

        state.reset();

        assert!(state.entries.is_empty());
        assert_eq!(state.filter_input.content(), "");
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll_offset, 0);
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

    mod grouping {
        use super::*;

        #[test]
        fn three_consecutive_identical_entries_become_one_group() {
            let state = make_state(vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 1"),
                make_entry("SELECT 1"),
            ]);

            let grouped = state.grouped_filtered_entries();

            assert_eq!(grouped.len(), 1);
            assert_eq!(grouped[0].entry.query, "SELECT 1");
            assert_eq!(grouped[0].count, 3);
        }

        #[test]
        fn non_consecutive_same_query_stays_separate() {
            let state = make_state(vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 2"),
                make_entry("SELECT 1"),
            ]);

            let grouped = state.grouped_filtered_entries();

            assert_eq!(grouped.len(), 3);
        }

        #[test]
        fn mixed_consecutive_and_unique() {
            let state = make_state(vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 1"),
                make_entry("SELECT 2"),
                make_entry("SELECT 3"),
                make_entry("SELECT 3"),
                make_entry("SELECT 3"),
            ]);

            // Reversed: [3,3,3,2,1,1] -> groups: [3(×3), 2(×1), 1(×2)]
            let grouped = state.grouped_filtered_entries();

            assert_eq!(grouped.len(), 3);
            assert_eq!(grouped[0].entry.query, "SELECT 3");
            assert_eq!(grouped[0].count, 3);
            assert_eq!(grouped[1].entry.query, "SELECT 2");
            assert_eq!(grouped[1].count, 1);
            assert_eq!(grouped[2].entry.query, "SELECT 1");
            assert_eq!(grouped[2].count, 2);
        }

        #[test]
        fn filter_removes_separator_merging_groups() {
            let mut state = make_state(vec![
                make_entry("SELECT * FROM users"),
                make_entry("INSERT INTO orders VALUES (1)"),
                make_entry("SELECT * FROM users"),
                make_entry("SELECT * FROM users"),
            ]);
            state.filter_input.set_content("users".to_string());

            let grouped = state.grouped_filtered_entries();

            // orders is filtered out, remaining 3 users become one group
            assert_eq!(grouped.len(), 1);
            assert_eq!(grouped[0].count, 3);
        }

        #[test]
        fn filter_preserves_non_consecutive_groups() {
            let mut state = make_state(vec![
                make_entry("SELECT * FROM users WHERE id=1"),
                make_entry("SELECT * FROM users WHERE id=2"),
                make_entry("INSERT INTO orders VALUES (1)"),
                make_entry("SELECT * FROM users WHERE id=1"),
            ]);
            state.filter_input.set_content("users".to_string());

            let grouped = state.grouped_filtered_entries();

            // Reversed + filtered: [id=1, id=2, id=1] -> 3 separate groups
            assert_eq!(grouped.len(), 3);
        }

        #[test]
        fn grouped_count_reflects_groups() {
            let state = make_state(vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 1"),
                make_entry("SELECT 2"),
            ]);

            assert_eq!(state.grouped_count(), 2);
        }

        #[test]
        fn clamped_selected_uses_grouped_count() {
            let state = QueryHistoryPickerState {
                entries: vec![
                    make_entry("SELECT 1"),
                    make_entry("SELECT 1"),
                    make_entry("SELECT 1"),
                ],
                selected: 5,
                ..Default::default()
            };

            assert_eq!(state.clamped_selected(), 0);
        }

        #[test]
        fn confirm_gets_representative_entry() {
            let state = make_state(vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 1"),
                make_entry("SELECT 1"),
            ]);

            let grouped = state.grouped_filtered_entries();
            let selected = state.clamped_selected();
            let query = grouped.get(selected).map(|g| g.entry.query.clone());

            assert_eq!(query, Some("SELECT 1".to_string()));
        }
    }
}
