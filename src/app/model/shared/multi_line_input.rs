use crate::app::update::action::CursorMove;

use super::text_input::TextInputState;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultiLineInputState {
    inner: TextInputState,
    scroll_row: usize,
}

impl MultiLineInputState {
    pub fn new(content: impl Into<String>, cursor: usize) -> Self {
        Self {
            inner: TextInputState::new(content, cursor),
            scroll_row: 0,
        }
    }

    // ── Accessors (delegated) ───────────────────────────────────────

    pub fn content(&self) -> &str {
        self.inner.content()
    }

    pub fn cursor(&self) -> usize {
        self.inner.cursor()
    }

    pub fn char_count(&self) -> usize {
        self.inner.char_count()
    }

    pub fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    // ── Text editing (delegated) ────────────────────────────────────

    pub fn insert_char(&mut self, c: char) {
        self.inner.insert_char(c);
    }

    pub fn insert_str(&mut self, text: &str) {
        self.inner.insert_str(text);
    }

    pub fn backspace(&mut self) {
        self.inner.backspace();
    }

    pub fn delete(&mut self) {
        self.inner.delete();
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
    }

    pub fn set_content_with_cursor(&mut self, s: String, cursor: usize) {
        let len = s.chars().count();
        self.inner.set_content(s);
        self.inner.set_cursor(cursor.min(len));
        self.scroll_row = 0;
    }

    pub fn clear(&mut self) {
        self.inner.clear();
        self.scroll_row = 0;
    }

    // ── Cursor movement (multi-line aware) ──────────────────────────

    pub fn move_cursor(&mut self, movement: CursorMove) {
        match movement {
            CursorMove::Left | CursorMove::Right => {
                self.inner.move_cursor(movement);
            }
            CursorMove::Up => {
                let (current_line, current_col) = self.current_line_col();
                if current_line > 0 {
                    let lines = self.line_spans();
                    let (prev_start, prev_len) = lines[current_line - 1];
                    self.set_cursor_raw(prev_start + current_col.min(prev_len));
                }
            }
            CursorMove::Down => {
                let (current_line, current_col) = self.current_line_col();
                let lines = self.line_spans();
                if current_line + 1 < lines.len() {
                    let (next_start, next_len) = lines[current_line + 1];
                    self.set_cursor_raw(next_start + current_col.min(next_len));
                }
            }
            CursorMove::Home => {
                let (current_line, _) = self.current_line_col();
                let lines = self.line_spans();
                if let Some((start, _)) = lines.get(current_line) {
                    self.set_cursor_raw(*start);
                }
            }
            CursorMove::End => {
                let (current_line, _) = self.current_line_col();
                let lines = self.line_spans();
                if let Some((start, len)) = lines.get(current_line) {
                    self.set_cursor_raw(start + len);
                }
            }
        }
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
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
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
        fn empty_string() {
            let s = ml("", 0);
            assert_eq!(s.cursor_to_position(), (0, 0));
        }

        #[test]
        fn single_line() {
            let s = ml("SELECT * FROM users", 7);
            assert_eq!(s.cursor_to_position(), (0, 7));
        }

        #[test]
        fn multiple_lines() {
            // "SELECT *\nFROM users\nWHERE id = 1"
            //  cursor at 17 → "FROM user" (8 chars of line0 + \n + 8 chars into line1)
            let s = ml("SELECT *\nFROM users\nWHERE id = 1", 17);
            assert_eq!(s.cursor_to_position(), (1, 8));
        }

        #[rstest]
        #[case("こんにちは\n世界", 5, (0, 5))]
        #[case("こんにちは\n世界", 6, (1, 0))]
        #[case("こんにちは\n世界", 7, (1, 1))]
        fn multibyte(
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
        fn left_right_single_line() {
            let mut s = ml("abc", 1);
            s.move_cursor(CursorMove::Left);
            assert_eq!(s.cursor(), 0);
            s.move_cursor(CursorMove::Right);
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn left_at_start_stays() {
            let mut s = ml("abc", 0);
            s.move_cursor(CursorMove::Left);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn right_at_end_stays() {
            let mut s = ml("abc", 3);
            s.move_cursor(CursorMove::Right);
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn up_from_second_line() {
            // "abc\ndef" → cursor at 5 (d=4, e=5) → col=1
            // Up → line 0, col 1 → cursor=1
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn up_from_first_line_stays() {
            let mut s = ml("abc\ndef", 1);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn down_from_first_line() {
            // "abc\ndef" → cursor at 1 → col=1
            // Down → line 1, col 1 → cursor=5
            let mut s = ml("abc\ndef", 1);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 5);
        }

        #[test]
        fn down_from_last_line_stays() {
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 5);
        }

        #[test]
        fn up_clamps_to_shorter_line() {
            // "ab\ncdef" → cursor at 7 (end of "cdef"), col=4
            // Up → line 0 has len 2, so col clamped to 2 → cursor=2
            let mut s = ml("ab\ncdef", 7);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn down_clamps_to_shorter_line() {
            // "cdef\nab" → cursor at 4 (end of "cdef"), col=4
            // Down → line 1 has len 2, so col clamped to 2 → cursor=7
            let mut s = ml("cdef\nab", 4);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn home_goes_to_line_start() {
            // "abc\ndef" → cursor at 5 (on 'e'), col=1
            // Home → start of line 1 → cursor=4
            let mut s = ml("abc\ndef", 5);
            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn end_goes_to_line_end() {
            // "abc\ndef" → cursor at 4 (on 'd'), col=0
            // End → end of line 1 → cursor=7
            let mut s = ml("abc\ndef", 4);
            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn home_on_first_line() {
            let mut s = ml("abc\ndef", 2);
            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn end_on_first_line() {
            let mut s = ml("abc\ndef", 0);
            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 3);
        }
    }

    // ── Edge cases: trailing newline, empty lines, consecutive newlines ──

    mod edge_case_tests {
        use super::*;

        #[test]
        fn trailing_newline_cursor_at_empty_last_line() {
            // "abc\n" → 2 lines: ("abc", 3) and ("", 0)
            // cursor at 4 → line 1, col 0
            let s = ml("abc\n", 4);
            assert_eq!(s.cursor_to_position(), (1, 0));
        }

        #[test]
        fn up_from_empty_trailing_line() {
            // "abc\n" → cursor at 4 (empty line 1)
            // Up → line 0, col 0.min(3) = 0 → cursor=0
            let mut s = ml("abc\n", 4);
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn down_to_empty_trailing_line() {
            // "abc\n" → cursor at 2 (col=2)
            // Down → line 1, col 2.min(0) = 0 → cursor=4
            let mut s = ml("abc\n", 2);
            s.move_cursor(CursorMove::Down);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn consecutive_newlines() {
            // "a\n\nb" → lines: ("a",1), ("",0), ("b",1)
            // cursor at 2 → line 1, col 0
            let s = ml("a\n\nb", 2);
            assert_eq!(s.cursor_to_position(), (1, 0));
        }

        #[test]
        fn up_down_through_empty_line() {
            // "abc\n\ndef" → lines: (0,3), (4,0), (5,3)
            // Start at cursor=6 (line 2, col 1 → 'e')
            let mut s = ml("abc\n\ndef", 6);
            assert_eq!(s.cursor_to_position(), (2, 1));

            // Up → line 1 (empty), col 1.min(0) = 0 → cursor=4
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 4);
            assert_eq!(s.cursor_to_position(), (1, 0));

            // Up again → line 0, col 0.min(3) = 0 → cursor=0
            s.move_cursor(CursorMove::Up);
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn home_end_on_empty_line() {
            // "abc\n\ndef" → cursor at 4 (empty line 1)
            let mut s = ml("abc\n\ndef", 4);

            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 4);

            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn cursor_just_before_newline() {
            // "abc\ndef" → cursor at 3 (on \n boundary, actually end of line 0)
            let s = ml("abc\ndef", 3);
            assert_eq!(s.cursor_to_position(), (0, 3));
        }

        #[test]
        fn cursor_just_after_newline() {
            // "abc\ndef" → cursor at 4 (start of line 1)
            let s = ml("abc\ndef", 4);
            assert_eq!(s.cursor_to_position(), (1, 0));
        }
    }

    // ── insert/edit operations ──────────────────────────────────────

    mod edit_tests {
        use super::*;

        #[test]
        fn insert_newline() {
            let mut s = ml("abcdef", 3);
            s.insert_newline();
            assert_eq!(s.content(), "abc\ndef");
            assert_eq!(s.cursor(), 4);
        }

        #[test]
        fn insert_tab() {
            let mut s = ml("abc", 3);
            s.insert_tab();
            assert_eq!(s.content(), "abc    ");
            assert_eq!(s.cursor(), 7);
        }

        #[test]
        fn backspace_at_newline_joins_lines() {
            // "abc\ndef" → cursor at 4 (start of "def")
            // backspace removes \n → "abcdef", cursor=3
            let mut s = ml("abc\ndef", 4);
            s.backspace();
            assert_eq!(s.content(), "abcdef");
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn delete_at_newline_joins_lines() {
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
        fn scroll_stays_when_cursor_visible() {
            let mut s = ml("line1\nline2\nline3", 0);
            s.update_scroll(3);
            assert_eq!(s.scroll_row(), 0);
        }

        #[test]
        fn scroll_follows_cursor_down() {
            // cursor on line 2 (index 2), visible_rows=2, scroll should advance
            let mut s = ml("line1\nline2\nline3", 12); // "line3" start
            s.update_scroll(2);
            assert_eq!(s.scroll_row(), 1); // row 2 - 2 + 1 = 1
        }

        #[test]
        fn scroll_follows_cursor_up() {
            let mut s = ml("line1\nline2\nline3", 0);
            s.scroll_row = 2;
            s.update_scroll(2);
            assert_eq!(s.scroll_row(), 0);
        }

        #[test]
        fn scroll_zero_visible_rows_noop() {
            let mut s = ml("line1\nline2", 6);
            s.scroll_row = 1;
            s.update_scroll(0);
            assert_eq!(s.scroll_row(), 1); // unchanged
        }
    }

    // ── set_content / clear ─────────────────────────────────────────

    mod content_management_tests {
        use super::*;

        #[test]
        fn set_content_resets_scroll_and_moves_cursor_to_end() {
            let mut s = ml("old\ncontent", 3);
            s.scroll_row = 5;

            s.set_content("new".to_string());

            assert_eq!(s.content(), "new");
            assert_eq!(s.cursor(), 3);
            assert_eq!(s.scroll_row(), 0);
        }

        #[test]
        fn clear_resets_everything() {
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
        fn ascii() {
            let s = ml("abcdef", 0);
            assert_eq!(s.char_to_byte_index(3), 3);
        }

        #[test]
        fn multibyte() {
            let s = ml("あいう", 0);
            // each hiragana is 3 bytes
            assert_eq!(s.char_to_byte_index(1), 3);
            assert_eq!(s.char_to_byte_index(2), 6);
        }

        #[test]
        fn past_end_returns_len() {
            let s = ml("abc", 0);
            assert_eq!(s.char_to_byte_index(100), 3);
        }
    }

    // ── multibyte multi-line ────────────────────────────────────────

    mod multibyte_multiline_tests {
        use super::*;

        #[test]
        fn up_down_with_multibyte_lines() {
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
        fn home_end_multibyte() {
            let mut s = ml("あいう\nかき", 5);

            s.move_cursor(CursorMove::Home);
            assert_eq!(s.cursor(), 4);

            s.move_cursor(CursorMove::End);
            assert_eq!(s.cursor(), 6);
        }
    }
}
