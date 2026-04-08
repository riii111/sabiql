use crate::app::update::action::CursorMove;

use super::text_input::{TextInputLike, TextInputState, next_word_start, previous_word_start};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultiLineInputState {
    inner: TextInputState,
    scroll_row: usize,
    preferred_col: Option<usize>,
}

impl MultiLineInputState {
    pub fn new(content: impl Into<String>, cursor: usize) -> Self {
        Self {
            inner: TextInputState::new(content, cursor),
            scroll_row: 0,
            preferred_col: None,
        }
    }

    pub fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    pub fn insert_newline(&mut self) {
        self.inner.insert_char('\n');
    }

    pub fn insert_tab(&mut self) {
        self.inner.insert_str("    ");
    }

    // ── Content management ──────────────────────────────────────────

    pub fn set_content(&mut self, s: String) {
        self.inner.set_content(s);
        self.scroll_row = 0;
        self.preferred_col = None;
    }

    pub fn set_content_with_cursor(&mut self, s: String, cursor: usize) {
        let len = s.chars().count();
        self.inner.set_content(s);
        self.inner.set_cursor(cursor.min(len));
        self.scroll_row = 0;
        self.preferred_col = None;
    }

    pub fn clear(&mut self) {
        self.inner.clear();
        self.scroll_row = 0;
        self.preferred_col = None;
    }

    // ── Cursor movement (multi-line aware) ──────────────────────────

    pub fn move_cursor(&mut self, movement: CursorMove) {
        match movement {
            CursorMove::Left | CursorMove::Right => {
                self.inner.move_cursor(movement);
                self.preferred_col = None;
            }
            CursorMove::Up => {
                let (current_line, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                if current_line > 0 {
                    let lines = self.line_spans();
                    let (prev_start, prev_len) = lines[current_line - 1];
                    self.set_cursor_raw(prev_start + preferred_col.min(prev_len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::Down => {
                let (current_line, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                let lines = self.line_spans();
                if current_line + 1 < lines.len() {
                    let (next_start, next_len) = lines[current_line + 1];
                    self.set_cursor_raw(next_start + preferred_col.min(next_len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::Home | CursorMove::LineStart => {
                let (current_line, _) = self.current_line_col();
                let lines = self.line_spans();
                if let Some((start, _)) = lines.get(current_line) {
                    self.set_cursor_raw(*start);
                }
                self.preferred_col = None;
            }
            CursorMove::End | CursorMove::LineEnd => {
                let (current_line, _) = self.current_line_col();
                let lines = self.line_spans();
                if let Some((start, len)) = lines.get(current_line) {
                    self.set_cursor_raw(start + len);
                }
                self.preferred_col = None;
            }
            CursorMove::WordForward => {
                let next = next_word_start(self.content(), self.cursor());
                self.set_cursor_raw(next);
                self.preferred_col = None;
            }
            CursorMove::WordBackward => {
                let previous = previous_word_start(self.content(), self.cursor());
                self.set_cursor_raw(previous);
                self.preferred_col = None;
            }
            CursorMove::BufferStart => {
                self.set_cursor_raw(0);
                self.preferred_col = None;
            }
            CursorMove::BufferEnd => {
                self.set_cursor_raw(self.char_count());
                self.preferred_col = None;
            }
            CursorMove::FirstLine => {
                let (_, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                let lines = self.line_spans();
                if let Some((start, len)) = lines.first() {
                    self.set_cursor_raw(start + preferred_col.min(*len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::LastLine => {
                let (_, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                let lines = self.line_spans();
                if let Some((start, len)) = lines.last() {
                    self.set_cursor_raw(start + preferred_col.min(*len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::ViewportTop | CursorMove::ViewportMiddle | CursorMove::ViewportBottom => {}
        }
    }

    pub fn move_cursor_to_viewport_position(&mut self, movement: CursorMove, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }

        let lines = self.line_spans();
        if lines.is_empty() {
            return;
        }

        let (_, current_col) = self.current_line_col();
        let preferred_col = self.preferred_col.unwrap_or(current_col);
        let target_row = match movement {
            CursorMove::ViewportTop => self.scroll_row,
            CursorMove::ViewportMiddle => self.scroll_row + visible_rows.saturating_sub(1) / 2,
            CursorMove::ViewportBottom => self.scroll_row + visible_rows.saturating_sub(1),
            _ => return,
        }
        .min(lines.len().saturating_sub(1));

        let (start, len) = lines[target_row];
        self.set_cursor_raw(start + preferred_col.min(len));
        self.preferred_col = Some(preferred_col);
    }

    // ── Coordinate conversion ───────────────────────────────────────

    pub fn cursor_to_position(&self) -> (usize, usize) {
        cursor_to_position_impl(self.content(), self.cursor())
    }

    // ── Scroll management ───────────────────────────────────────────

    pub fn update_scroll(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        let (row, _) = self.cursor_to_position();
        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + visible_rows {
            self.scroll_row = row - visible_rows + 1;
        }
    }

    // ── Byte conversion (for CompletionAccept etc.) ─────────────────

    pub fn char_to_byte_index(&self, char_idx: usize) -> usize {
        char_to_byte_index_impl(self.content(), char_idx)
    }

    // ── Internal helpers ────────────────────────────────────────────

    fn line_spans(&self) -> Vec<(usize, usize)> {
        let content = self.content();
        let mut result = Vec::new();
        let mut start = 0;
        for line in content.split('\n') {
            let len = line.chars().count();
            result.push((start, len));
            start += len + 1; // +1 for '\n'
        }
        result
    }

    fn current_line_col(&self) -> (usize, usize) {
        let cursor = self.cursor();
        let lines = self.line_spans();
        for (i, (start, len)) in lines.iter().enumerate() {
            if cursor >= *start && cursor <= start + len {
                return (i, cursor - start);
            }
        }
        (0, cursor)
    }

    fn set_cursor_raw(&mut self, pos: usize) {
        let clamped = pos.min(self.char_count());
        // viewport reset by set_cursor is acceptable: MultiLineInputState doesn't use inner's viewport
        self.inner.set_cursor(clamped);
    }
}

impl TextInputLike for MultiLineInputState {
    fn text_input(&self) -> &TextInputState {
        &self.inner
    }

    fn text_input_mut(&mut self) -> &mut TextInputState {
        &mut self.inner
    }
}

fn cursor_to_position_impl(content: &str, cursor_pos: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;

    for (current_pos, ch) in content.chars().enumerate() {
        if current_pos >= cursor_pos {
            break;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (row, col)
}

fn char_to_byte_index_impl(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn ml(content: &str, cursor: usize) -> MultiLineInputState {
        MultiLineInputState::new(content, cursor)
    }

    // ── cursor_to_position ──────────────────────────────────────────

    mod cursor_to_position_tests {
        use super::*;

        #[test]
        fn empty_string_returns_origin() {
            let s = ml("", 0);
            assert_eq!(s.cursor_to_position(), (0, 0));
        }

        #[test]
        fn single_line_returns_correct_col() {
            let s = ml("SELECT * FROM users", 7);
            assert_eq!(s.cursor_to_position(), (0, 7));
        }

        #[test]
        fn multiline_returns_correct_row_and_col() {
            // "SELECT *\nFROM users\nWHERE id = 1"
            //  cursor at 17 → "FROM user" (8 chars of line0 + \n + 8 chars into line1)
            let s = ml("SELECT *\nFROM users\nWHERE id = 1", 17);
            assert_eq!(s.cursor_to_position(), (1, 8));
        }

        #[rstest]
        #[case("こんにちは\n世界", 5, (0, 5))]
        #[case("こんにちは\n世界", 6, (1, 0))]
        #[case("こんにちは\n世界", 7, (1, 1))]
        fn multibyte_returns_correct_row_and_col(
            #[case] content: &str,
            #[case] cursor: usize,
            #[case] expected: (usize, usize),
        ) {
            let s = ml(content, cursor);
            assert_eq!(s.cursor_to_position(), expected);
        }
    }

    // ── move_cursor ─────────────────────────────────────────────────

    mod move_cursor_tests {
        use super::*;

        #[test]
        fn left_right_moves_cursor_by_one() {
            let mut s = ml("abc", 1);
            s.move_cursor(CursorMove::Left);
            assert_eq!(s.cursor(), 0);
            s.move_cursor(CursorMove::Right);
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn left_at_start_returns_zero() {
            let mut s = ml("abc", 0);
            s.move_cursor(CursorMove::Left);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn right_at_end_returns_unchanged() {
            let mut s = ml("abc", 3);
            s.move_cursor(CursorMove::Right);
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn up_from_second_line_returns_same_col_in_first() {
            // "abc\ndef" → cursor at 5 (d=4, e=5) → col=1
            // Up → line 0, col 1 → cursor=1
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn up_from_first_line_returns_unchanged() {
            let mut s = ml("abc\ndef", 1);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn down_from_first_line_returns_same_col_in_second() {
            // "abc\ndef" → cursor at 1 → col=1
            // Down → line 1, col 1 → cursor=5
            let mut s = ml("abc\ndef", 1);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 5);
        }

        #[test]
        fn down_from_last_line_returns_unchanged() {
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 5);
        }

        #[test]
        fn up_clamps_col_to_shorter_line_length() {
            // "ab\ncdef" → cursor at 7 (end of "cdef"), col=4
            // Up → line 0 has len 2, so col clamped to 2 → cursor=2
            let mut s = ml("ab\ncdef", 7);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn down_clamps_col_to_shorter_line_length() {
            // "cdef\nab" → cursor at 4 (end of "cdef"), col=4
            // Down → line 1 has len 2, so col clamped to 2 → cursor=7
            let mut s = ml("cdef\nab", 4);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn home_returns_line_start() {
            // "abc\ndef" → cursor at 5 (on 'e'), col=1
            // Home → start of line 1 → cursor=4
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn end_returns_line_end() {
            // "abc\ndef" → cursor at 4 (on 'd'), col=0
            // End → end of line 1 → cursor=7
            let mut s = ml("abc\ndef", 4);
            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn home_on_first_line_returns_zero() {
            let mut s = ml("abc\ndef", 2);
            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn end_on_first_line_returns_line_length() {
            let mut s = ml("abc\ndef", 0);
            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn line_start_returns_current_line_start() {
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::LineStart);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn line_end_returns_current_line_end() {
            let mut s = ml("abc\ndef", 4);
            s.move_cursor(CursorMove::LineEnd);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn word_forward_moves_within_line() {
            let mut s = ml("SELECT users", 0);
            s.move_cursor(CursorMove::WordForward);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn word_backward_moves_to_start_of_current_word() {
            let mut s = ml("SELECT users", 10);
            s.move_cursor(CursorMove::WordBackward);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn word_forward_crosses_punctuation_boundary() {
            let mut s = ml("foo(bar)", 0);
            s.move_cursor(CursorMove::WordForward);
            assert_eq!(s.cursor(), 3);

            s.move_cursor(CursorMove::WordForward);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn word_backward_crosses_punctuation_boundary() {
            let mut s = ml("foo(bar)", 7);
            s.move_cursor(CursorMove::WordBackward);
            assert_eq!(s.cursor(), 4);

            s.move_cursor(CursorMove::WordBackward);
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn word_forward_crosses_whitespace_and_newline() {
            let mut s = ml("foo \n  bar", 0);
            s.move_cursor(CursorMove::WordForward);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn word_backward_crosses_whitespace_and_newline() {
            let mut s = ml("foo \n  bar", 10);
            s.move_cursor(CursorMove::WordBackward);
            assert_eq!(s.cursor(), 7);

            s.move_cursor(CursorMove::WordBackward);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn word_forward_at_end_returns_unchanged() {
            let mut s = ml("foo", 3);
            s.move_cursor(CursorMove::WordForward);
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn word_backward_at_start_returns_unchanged() {
            let mut s = ml("foo", 0);
            s.move_cursor(CursorMove::WordBackward);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn buffer_start_moves_to_start_of_buffer() {
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::BufferStart);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn buffer_end_moves_to_end_of_buffer() {
            let mut s = ml("abc\ndef", 1);
            s.move_cursor(CursorMove::BufferEnd);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn first_line_preserves_column() {
            let mut s = ml("abcd\nxy\nwxyz12", 10);
            s.move_cursor(CursorMove::FirstLine);
            assert_eq!(s.cursor_to_position(), (0, 2));
        }

        #[test]
        fn first_line_clamps_to_line_length() {
            let mut s = ml("xy\nabcdef", 8);
            s.move_cursor(CursorMove::FirstLine);
            assert_eq!(s.cursor_to_position(), (0, 2));
        }

        #[test]
        fn last_line_preserves_column() {
            let mut s = ml("abcd\nxy\nwxyz12", 2);
            s.move_cursor(CursorMove::LastLine);
            assert_eq!(s.cursor_to_position(), (2, 2));
        }

        #[test]
        fn last_line_clamps_to_line_length() {
            let mut s = ml("abcdef\nxy", 5);
            s.move_cursor(CursorMove::LastLine);
            assert_eq!(s.cursor_to_position(), (1, 2));
        }

        #[test]
        fn preferred_column_restored_long_to_short_line() {
            let mut s = ml("abcdefghij\nxy", 5);

            s.move_cursor(CursorMove::LastLine);
            assert_eq!(s.cursor_to_position(), (1, 2));

            s.move_cursor(CursorMove::FirstLine);
            assert_eq!(s.cursor_to_position(), (0, 5));
        }

        #[test]
        fn preferred_column_restored_short_to_long_line() {
            let mut s = ml("xy\nabcdefghij", 8);

            s.move_cursor(CursorMove::FirstLine);
            assert_eq!(s.cursor_to_position(), (0, 2));

            s.move_cursor(CursorMove::LastLine);
            assert_eq!(s.cursor_to_position(), (1, 5));
        }
    }

    // ── Edge cases: trailing newline, empty lines, consecutive newlines ──

    mod edge_case_tests {
        use super::*;

        #[test]
        fn trailing_newline_returns_next_row_origin() {
            // "abc\n" → 2 lines: ("abc", 3) and ("", 0)
            // cursor at 4 → line 1, col 0
            let s = ml("abc\n", 4);
            assert_eq!(s.cursor_to_position(), (1, 0));
        }

        #[test]
        fn up_from_empty_trailing_line_returns_prev_line_origin() {
            // "abc\n" → cursor at 4 (empty line 1)
            // Up → line 0, col 0.min(3) = 0 → cursor=0
            let mut s = ml("abc\n", 4);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn down_to_empty_trailing_line_returns_next_row_origin() {
            // "abc\n" → cursor at 2 (col=2)
            // Down → line 1, col 2.min(0) = 0 → cursor=4
            let mut s = ml("abc\n", 2);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn consecutive_newlines_returns_middle_row() {
            // "a\n\nb" → lines: ("a",1), ("",0), ("b",1)
            // cursor at 2 → line 1, col 0
            let s = ml("a\n\nb", 2);
            assert_eq!(s.cursor_to_position(), (1, 0));
        }

        #[test]
        fn up_through_empty_line_restores_preferred_column() {
            // "abc\n\ndef" → lines: (0,3), (4,0), (5,3)
            // Start at cursor=6 (line 2, col 1 → 'e')
            let mut s = ml("abc\n\ndef", 6);
            assert_eq!(s.cursor_to_position(), (2, 1));

            // Up → line 1 (empty), col 1.min(0) = 0 → cursor=4
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 4);
            assert_eq!(s.cursor_to_position(), (1, 0));

            // Up again → line 0, restore preferred col 1 → cursor=1
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 1);
            assert_eq!(s.cursor_to_position(), (0, 1));
        }

        #[test]
        fn home_end_on_empty_line_returns_same_position() {
            // "abc\n\ndef" → cursor at 4 (empty line 1)
            let mut s = ml("abc\n\ndef", 4);

            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 4);

            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn cursor_before_newline_returns_end_of_current_line() {
            // "abc\ndef" → cursor at 3 (on \n boundary, actually end of line 0)
            let s = ml("abc\ndef", 3);
            assert_eq!(s.cursor_to_position(), (0, 3));
        }

        #[test]
        fn cursor_after_newline_returns_start_of_next_line() {
            // "abc\ndef" → cursor at 4 (start of line 1)
            let s = ml("abc\ndef", 4);
            assert_eq!(s.cursor_to_position(), (1, 0));
        }
    }

    // ── insert/edit operations ──────────────────────────────────────

    mod edit_tests {
        use super::*;

        #[test]
        fn insert_newline_splits_content() {
            let mut s = ml("abcdef", 3);
            s.insert_newline();
            assert_eq!(s.content(), "abc\ndef");
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn insert_tab_adds_four_spaces() {
            let mut s = ml("abc", 3);
            s.insert_tab();
            assert_eq!(s.content(), "abc    ");
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn backspace_at_newline_joins_adjacent_lines() {
            // "abc\ndef" → cursor at 4 (start of "def")
            // backspace removes \n → "abcdef", cursor=3
            let mut s = ml("abc\ndef", 4);
            s.backspace();
            assert_eq!(s.content(), "abcdef");
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn delete_at_newline_joins_adjacent_lines() {
            // "abc\ndef" → cursor at 3 (end of "abc", on \n)
            // delete removes \n → "abcdef", cursor=3
            let mut s = ml("abc\ndef", 3);
            s.delete();
            assert_eq!(s.content(), "abcdef");
            assert_eq!(s.cursor(), 3);
        }
    }

    // ── scroll ──────────────────────────────────────────────────────

    mod scroll_tests {
        use super::*;

        #[test]
        fn cursor_within_viewport_returns_unchanged_scroll() {
            let mut s = ml("line1\nline2\nline3", 0);
            s.update_scroll(3);
            assert_eq!(s.scroll_row(), 0);
        }

        #[test]
        fn cursor_below_viewport_advances_scroll() {
            // cursor on line 2 (index 2), visible_rows=2, scroll should advance
            let mut s = ml("line1\nline2\nline3", 12); // "line3" start
            s.update_scroll(2);
            assert_eq!(s.scroll_row(), 1); // row 2 - 2 + 1 = 1
        }

        #[test]
        fn cursor_above_viewport_retreats_scroll() {
            let mut s = ml("line1\nline2\nline3", 0);
            s.scroll_row = 2;
            s.update_scroll(2);
            assert_eq!(s.scroll_row(), 0);
        }

        #[test]
        fn zero_visible_rows_returns_unchanged_scroll() {
            let mut s = ml("line1\nline2", 6);
            s.scroll_row = 1;
            s.update_scroll(0);
            assert_eq!(s.scroll_row(), 1); // unchanged
        }

        #[test]
        fn viewport_top_preserves_column() {
            let mut s = ml("aa\nbb\ncc\ndd", 10);
            s.scroll_row = 1;
            s.move_cursor_to_viewport_position(CursorMove::ViewportTop, 3);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn viewport_middle_moves_cursor_to_middle_visible_row_preserving_column() {
            let mut s = ml("aa\nbb\ncc\ndd\nee", 13);
            s.scroll_row = 1;
            s.move_cursor_to_viewport_position(CursorMove::ViewportMiddle, 3);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn viewport_bottom_moves_cursor_to_bottom_visible_row_preserving_column() {
            let mut s = ml("aa\nbb\ncc\ndd\nee", 1);
            s.scroll_row = 1;
            s.move_cursor_to_viewport_position(CursorMove::ViewportBottom, 3);
            assert_eq!(s.cursor(), 10);
        }
    }

    // ── set_content / clear ─────────────────────────────────────────

    mod content_management_tests {
        use super::*;

        #[test]
        fn set_content_resets_scroll_and_sets_cursor_to_end() {
            let mut s = ml("old\ncontent", 3);
            s.scroll_row = 5;

            s.set_content("new".to_string());

            assert_eq!(s.content(), "new");
            assert_eq!(s.cursor(), 3);
            assert_eq!(s.scroll_row(), 0);
        }

        #[test]
        fn set_content_with_cursor_sets_exact_position() {
            let mut s = ml("old\ncontent", 3);
            s.scroll_row = 5;

            s.set_content_with_cursor("new\nvalue".to_string(), 4);

            assert_eq!(s.content(), "new\nvalue");
            assert_eq!(s.cursor(), 4);
            assert_eq!(s.scroll_row(), 0);
        }

        #[test]
        fn set_content_with_cursor_clamps_past_end() {
            let mut s = ml("x", 0);

            s.set_content_with_cursor("ab".to_string(), 100);

            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn clear_resets_all_fields() {
            let mut s = ml("multi\nline", 8);
            s.scroll_row = 3;

            s.clear();

            assert_eq!(s.content(), "");
            assert_eq!(s.cursor(), 0);
            assert_eq!(s.scroll_row(), 0);
        }
    }

    // ── char_to_byte_index ──────────────────────────────────────────

    mod byte_index_tests {
        use super::*;

        #[test]
        fn ascii_returns_same_index() {
            let s = ml("abcdef", 0);
            assert_eq!(s.char_to_byte_index(3), 3);
        }

        #[test]
        fn multibyte_returns_correct_byte_indices() {
            let s = ml("あいう", 0);
            // each hiragana is 3 bytes
            assert_eq!(s.char_to_byte_index(1), 3);
            assert_eq!(s.char_to_byte_index(2), 6);
        }

        #[test]
        fn past_end_returns_content_byte_len() {
            let s = ml("abc", 0);
            assert_eq!(s.char_to_byte_index(100), 3);
        }
    }

    // ── multibyte multi-line ────────────────────────────────────────

    mod multibyte_multiline_tests {
        use super::*;

        #[test]
        fn multibyte_up_down_preserves_col() {
            // "あいう\nかき" → lines: (0,3), (4,2)
            // cursor at 5 (line 1, col 1 → 'き')
            let mut s = ml("あいう\nかき", 5);
            assert_eq!(s.cursor_to_position(), (1, 1));

            // Up → line 0, col 1.min(3) = 1 → cursor=1
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 1);

            // Down → line 1, col 1.min(2) = 1 → cursor=5
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 5);
        }

        #[test]
        fn multibyte_home_end_returns_line_boundaries() {
            let mut s = ml("あいう\nかき", 5);

            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 4);

            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 6);
        }
    }
}
