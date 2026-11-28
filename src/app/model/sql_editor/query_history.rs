use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use unicode_segmentation::UnicodeSegmentation;

use crate::domain::query_history::QueryHistoryEntry;
use crate::model::shared::picker::PickerState;
use crate::model::shared::text_input::TextEdit;
use crate::model::shared::text_input::TextInputState;

#[derive(Debug, Clone, Default)]
pub struct QueryHistoryPickerState {
    entries: Vec<QueryHistoryEntry>,
    picker: PickerState,
}

pub struct GroupedEntry<'a> {
    pub entry: &'a QueryHistoryEntry,
    pub count: usize,
    pub match_byte_offsets: Vec<usize>,
}

impl QueryHistoryPickerState {
    pub fn entries(&self) -> &[QueryHistoryEntry] {
        &self.entries
    }

    pub fn filter_input(&self) -> &TextInputState {
        self.picker.filter_input()
    }

    pub fn selected(&self) -> usize {
        self.picker.selected()
    }

    pub fn scroll_offset(&self) -> usize {
        self.picker.scroll_offset()
    }

    pub fn pane_height(&self) -> u16 {
        self.picker.pane_height()
    }

    pub fn filter_visible_width(&self) -> usize {
        self.picker.filter_visible_width()
    }

    pub fn set_pane_height(&mut self, height: u16) {
        self.picker.set_pane_height(height);
    }

    pub fn set_filter_visible_width(&mut self, width: usize) {
        self.picker.set_filter_visible_width(width);
    }

    pub fn replace_entries(&mut self, entries: &[QueryHistoryEntry]) {
        self.entries.clear();
        self.entries.extend_from_slice(entries);
        self.picker.reset();
    }

    pub fn reset(&mut self) {
        self.entries.clear();
        self.picker.clear_filter_and_reset();
    }

    pub fn insert_filter_str(&mut self, text: &str) {
        self.picker.insert_filter_str(text);
    }

    pub fn edit_filter<R>(&mut self, edit: impl FnOnce(&mut TextInputState) -> R) -> R {
        self.picker.edit_filter(edit)
    }

    pub fn apply_filter_edit(&mut self, edit: TextEdit, yank_text: Option<&str>) -> String {
        self.picker.apply_filter_edit(edit, yank_text)
    }

    pub fn reset_selection(&mut self) {
        self.picker.reset();
    }

    pub fn select_next(&mut self) {
        let count = self.grouped_count();
        if count > 0 {
            self.picker
                .set_selection((self.clamped_selected() + 1).min(count - 1));
        }
    }

    pub fn select_previous(&mut self) {
        self.picker
            .set_selection(self.clamped_selected().saturating_sub(1));
    }

    pub fn grouped_filtered_entries(&self) -> Vec<GroupedEntry<'_>> {
        let filter = self.picker.filter_input().content();
        let pattern = (!filter.is_empty())
            .then(|| Pattern::parse(filter, CaseMatching::Ignore, Normalization::Smart));
        let mut matcher = None;
        let mut groups: Vec<GroupedEntry<'_>> = Vec::new();

        for entry in self.entries.iter().rev() {
            let match_byte_offsets = if let Some(pattern) = pattern.as_ref() {
                let mut indices = Vec::new();
                let mut buf = Vec::new();
                let haystack = Utf32Str::new(&entry.query, &mut buf);
                let matcher = matcher.get_or_insert_with(|| Matcher::new(Config::DEFAULT));
                if pattern.indices(haystack, matcher, &mut indices).is_none() {
                    continue;
                }
                match_byte_offsets(&entry.query, haystack, indices)
            } else {
                Vec::new()
            };

            if let Some(last) = groups.last_mut().filter(|g| g.entry.query == entry.query) {
                last.count += 1;
                continue;
            }
            groups.push(GroupedEntry {
                entry,
                count: 1,
                match_byte_offsets,
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
            self.picker.selected().min(count.saturating_sub(1))
        }
    }
}

fn match_byte_offsets(query: &str, haystack: Utf32Str<'_>, mut indices: Vec<u32>) -> Vec<usize> {
    if matches!(haystack, Utf32Str::Ascii(_)) {
        return indices.into_iter().map(|index| index as usize).collect();
    }

    indices.sort_unstable();
    query
        .grapheme_indices(true)
        .enumerate()
        .filter_map(|(index, (offset, _))| {
            indices
                .binary_search(&(index as u32))
                .is_ok()
                .then_some(offset)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ConnectionId;
    use crate::model::shared::cursor::CursorMove;

    impl QueryHistoryPickerState {
        pub(crate) fn set_selection_for_test(&mut self, selected: usize) {
            self.picker.selected = selected;
        }
    }

    fn state_with_selection() -> QueryHistoryPickerState {
        QueryHistoryPickerState {
            picker: PickerState {
                selected: 3,
                scroll_offset: 2,

                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn insert_filter_char_resets_selection() {
        let mut state = state_with_selection();

        state.apply_filter_edit(TextEdit::Insert('a'), None);

        assert_eq!(state.picker.filter_input.content(), "a");
        assert_eq!(state.picker.selected, 0);
        assert_eq!(state.picker.scroll_offset, 0);
    }

    #[test]
    fn insert_filter_str_strips_newlines_and_resets_selection() {
        let mut state = state_with_selection();

        state.insert_filter_str("a\nb\rc");

        assert_eq!(state.picker.filter_input.content(), "abc");
        assert_eq!(state.picker.selected, 0);
        assert_eq!(state.picker.scroll_offset, 0);
    }

    #[test]
    fn backspace_filter_resets_selection() {
        let mut state = QueryHistoryPickerState {
            picker: PickerState {
                filter_input: TextInputState::new("abc", 3),

                ..state_with_selection().picker
            },
            ..state_with_selection()
        };

        state.apply_filter_edit(TextEdit::Backspace, None);

        assert_eq!(state.picker.filter_input.content(), "ab");
        assert_eq!(state.picker.selected, 0);
        assert_eq!(state.picker.scroll_offset, 0);
    }

    #[test]
    fn move_filter_cursor_preserves_selection() {
        let mut state = QueryHistoryPickerState {
            picker: PickerState {
                filter_input: TextInputState::new("abc", 3),

                ..state_with_selection().picker
            },
            ..state_with_selection()
        };

        state.apply_filter_edit(TextEdit::MoveCursor(CursorMove::Left), None);

        assert_eq!(state.picker.selected, 3);
        assert_eq!(state.picker.scroll_offset, 2);
    }

    #[test]
    fn filter_input_updates_viewport() {
        let mut state = QueryHistoryPickerState {
            picker: PickerState {
                filter_visible_width: 3,

                ..Default::default()
            },
            ..Default::default()
        };

        state.insert_filter_str("abcd");

        assert_eq!(state.picker.filter_input.content(), "abcd");
        assert_eq!(state.picker.filter_input.viewport_offset(), 2);
    }

    fn make_entry(query: &str) -> QueryHistoryEntry {
        use crate::domain::query_history::QueryResultStatus;
        QueryHistoryEntry::new_with_database(
            query.to_string(),
            "2026-03-13T12:00:00Z".to_string(),
            ConnectionId::from_string("test-conn"),
            None,
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

        let grouped = state.grouped_filtered_entries();

        assert_eq!(grouped.len(), 3);
        assert_eq!(grouped[0].entry.query, "SELECT 3");
        assert_eq!(grouped[1].entry.query, "SELECT 2");
        assert_eq!(grouped[2].entry.query, "SELECT 1");
    }

    #[test]
    fn fuzzy_filter_matches_partial_query() {
        let mut state = make_state(vec![
            make_entry("SELECT * FROM users"),
            make_entry("INSERT INTO orders VALUES (1)"),
            make_entry("SELECT count(*) FROM users"),
        ]);
        state.picker.filter_input.set_content("users".to_string());

        let grouped = state.grouped_filtered_entries();

        assert_eq!(grouped.len(), 2);
        assert!(grouped.iter().all(|g| g.entry.query.contains("users")));
    }

    #[test]
    fn negative_filter_distinguishes_newlines_from_literal_spaces() {
        let mut state = make_state(vec![make_entry("SELECT\n42"), make_entry("SELECT 42")]);
        state.insert_filter_str(r"!SELECT\ 42");

        let grouped = state.grouped_filtered_entries();

        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].entry.query, "SELECT\n42");
    }

    #[test]
    fn filter_is_case_insensitive() {
        let mut state = make_state(vec![make_entry("SELECT * FROM Users")]);
        state.picker.filter_input.set_content("users".to_string());

        let grouped = state.grouped_filtered_entries();

        assert_eq!(grouped.len(), 1);
    }

    #[test]
    fn reset_clears_all_state() {
        let mut state = QueryHistoryPickerState {
            entries: vec![make_entry("SELECT 1")],

            picker: PickerState {
                selected: 5,
                scroll_offset: 3,

                ..Default::default()
            },
            ..Default::default()
        };
        state.picker.filter_input.set_content("test".to_string());

        state.reset();

        assert!(state.entries.is_empty());
        assert_eq!(state.picker.filter_input.content(), "");
        assert_eq!(state.picker.selected, 0);
        assert_eq!(state.picker.scroll_offset, 0);
    }

    #[test]
    fn clamped_selected_with_empty_entries() {
        let state = QueryHistoryPickerState {
            picker: PickerState {
                selected: 5,

                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(state.clamped_selected(), 0);
    }

    #[test]
    fn clamped_selected_clamps_to_last_index() {
        let state = QueryHistoryPickerState {
            entries: vec![make_entry("SELECT 1"), make_entry("SELECT 2")],

            picker: PickerState {
                selected: 10,

                ..Default::default()
            },
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

            picker: PickerState {
                selected: 1,

                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(state.clamped_selected(), 1);
    }

    #[test]
    fn select_next_updates_scroll_offset() {
        let mut state = QueryHistoryPickerState {
            entries: vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 2"),
                make_entry("SELECT 3"),
                make_entry("SELECT 4"),
            ],

            picker: PickerState {
                pane_height: 2,

                ..Default::default()
            },
            ..Default::default()
        };

        state.select_next();
        state.select_next();

        assert_eq!(state.selected(), 2);
        assert_eq!(state.scroll_offset(), 1);
    }

    #[test]
    fn select_previous_updates_scroll_offset() {
        let mut state = QueryHistoryPickerState {
            entries: vec![
                make_entry("SELECT 1"),
                make_entry("SELECT 2"),
                make_entry("SELECT 3"),
                make_entry("SELECT 4"),
            ],

            picker: PickerState {
                selected: 3,
                scroll_offset: 2,
                pane_height: 2,

                ..Default::default()
            },
            ..Default::default()
        };

        state.select_previous();
        state.select_previous();

        assert_eq!(state.selected(), 1);
        assert_eq!(state.scroll_offset(), 1);
    }

    #[test]
    fn select_previous_starts_from_clamped_selection() {
        let mut state = QueryHistoryPickerState {
            entries: vec![make_entry("SELECT 1"), make_entry("SELECT 2")],

            picker: PickerState {
                selected: 10,
                pane_height: 5,

                ..Default::default()
            },
            ..Default::default()
        };

        state.select_previous();

        assert_eq!(state.selected(), 0);
        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn no_matches_returns_empty() {
        let mut state = make_state(vec![make_entry("SELECT 1")]);
        state
            .picker
            .filter_input
            .set_content("xyz_no_match".to_string());

        let grouped = state.grouped_filtered_entries();

        assert!(grouped.is_empty());
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
            state.picker.filter_input.set_content("users".to_string());

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
            state.picker.filter_input.set_content("users".to_string());

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

                picker: PickerState {
                    selected: 5,

                    ..Default::default()
                },
                ..Default::default()
            };

            assert_eq!(state.clamped_selected(), 0);
        }
    }
}
