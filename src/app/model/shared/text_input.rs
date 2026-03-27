use crate::app::update::action::CursorMove;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInputState {
    content: String,
    cursor: usize,
    viewport_offset: usize,
}

impl TextInputState {
    pub fn new(content: impl Into<String>, cursor: usize) -> Self {
        let content = content.into();
        let len = content.chars().count();
        Self {
            content,
            cursor: cursor.min(len),
            viewport_offset: 0,
        }
    }

    pub fn with_viewport(
        content: impl Into<String>,
        cursor: usize,
        viewport_offset: usize,
    ) -> Self {
        let content = content.into();
        let len = content.chars().count();
        Self {
            content,
            cursor: cursor.min(len),
            viewport_offset,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.char_count());
        self.viewport_offset = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        let byte_idx = char_to_byte_index(&self.content, self.cursor);
        self.content.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, text: &str) {
        let byte_idx = char_to_byte_index(&self.content, self.cursor);
        self.content.insert_str(byte_idx, text);
        self.cursor += text.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.cursor - 1;
        let start = char_to_byte_index(&self.content, prev);
        let end = char_to_byte_index(&self.content, self.cursor);
        self.content.drain(start..end);
        self.cursor = prev;
    }

    pub fn delete(&mut self) {
        let len = self.char_count();
        if self.cursor >= len {
            return;
        }
        let start = char_to_byte_index(&self.content, self.cursor);
        let end = char_to_byte_index(&self.content, self.cursor + 1);
        self.content.drain(start..end);
    }

    pub fn move_cursor(&mut self, movement: CursorMove) {
        match movement {
            CursorMove::Left => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            CursorMove::Right => {
                let len = self.char_count();
                if self.cursor < len {
                    self.cursor += 1;
                }
            }
            CursorMove::Home => {
                self.cursor = 0;
            }
            CursorMove::End => {
                self.cursor = self.char_count();
            }
            CursorMove::Up | CursorMove::Down => {}
        }
    }

    pub fn update_viewport(&mut self, visible_width: usize) {
        if visible_width == 0 {
            self.viewport_offset = 0;
            return;
        }

        // █ occupies one terminal cell at end-of-input; shrink effective width to keep it visible
        let effective_width = if self.cursor == self.char_count() {
            visible_width.saturating_sub(1)
        } else {
            visible_width
        };

        if effective_width == 0 {
            self.viewport_offset = self.cursor;
            return;
        }

        if self.cursor < self.viewport_offset {
            self.viewport_offset = self.cursor;
        } else if self.cursor >= self.viewport_offset + effective_width {
            self.viewport_offset = self.cursor - effective_width + 1;
        }
    }

    pub fn set_content(&mut self, s: String) {
        let len = s.chars().count();
        self.content = s;
        self.cursor = len;
        self.viewport_offset = 0;
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
        self.viewport_offset = 0;
    }

    pub fn char_count(&self) -> usize {
        self.content.chars().count()
    }
}

fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(content: &str, cursor: usize) -> TextInputState {
        TextInputState::new(content, cursor)
    }

    mod insert_char_tests {
        use super::*;

        #[test]
        fn insert_at_empty() {
            let mut s = TextInputState::default();

            s.insert_char('a');

            assert_eq!(s.content(), "a");
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn insert_at_end() {
            let mut s = state_with("ab", 2);

            s.insert_char('c');

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn insert_at_beginning() {
            let mut s = state_with("bc", 0);

            s.insert_char('a');

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn insert_at_middle() {
            let mut s = state_with("ac", 1);

            s.insert_char('b');

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn insert_multibyte() {
            let mut s = state_with("あう", 1);

            s.insert_char('い');

            assert_eq!(s.content(), "あいう");
            assert_eq!(s.cursor(), 2);
        }
    }

    mod insert_str_tests {
        use super::*;

        #[test]
        fn insert_str_at_beginning() {
            let mut s = state_with("cd", 0);

            s.insert_str("ab");

            assert_eq!(s.content(), "abcd");
            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn insert_str_at_middle() {
            let mut s = state_with("ad", 1);

            s.insert_str("bc");

            assert_eq!(s.content(), "abcd");
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn insert_str_multibyte() {
            let mut s = state_with("あえ", 1);

            s.insert_str("いう");

            assert_eq!(s.content(), "あいうえ");
            assert_eq!(s.cursor(), 3);
        }
    }

    mod backspace_tests {
        use super::*;

        #[test]
        fn backspace_at_start_is_noop() {
            let mut s = state_with("abc", 0);

            s.backspace();

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn backspace_at_end() {
            let mut s = state_with("abc", 3);

            s.backspace();

            assert_eq!(s.content(), "ab");
            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn backspace_at_middle() {
            let mut s = state_with("abc", 2);

            s.backspace();

            assert_eq!(s.content(), "ac");
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn backspace_empty_string() {
            let mut s = TextInputState::default();

            s.backspace();

            assert_eq!(s.content(), "");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn backspace_multibyte() {
            let mut s = state_with("あいう", 2);

            s.backspace();

            assert_eq!(s.content(), "あう");
            assert_eq!(s.cursor(), 1);
        }
    }

    mod delete_tests {
        use super::*;

        #[test]
        fn delete_at_end_is_noop() {
            let mut s = state_with("abc", 3);

            s.delete();

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn delete_at_beginning() {
            let mut s = state_with("abc", 0);

            s.delete();

            assert_eq!(s.content(), "bc");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn delete_at_middle() {
            let mut s = state_with("abc", 1);

            s.delete();

            assert_eq!(s.content(), "ac");
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn delete_empty_string() {
            let mut s = TextInputState::default();

            s.delete();

            assert_eq!(s.content(), "");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn delete_multibyte() {
            let mut s = state_with("あいう", 1);

            s.delete();

            assert_eq!(s.content(), "あう");
            assert_eq!(s.cursor(), 1);
        }
    }

    mod move_cursor_tests {
        use super::*;

        #[test]
        fn move_left() {
            let mut s = state_with("abc", 2);

            s.move_cursor(CursorMove::Left);

            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn move_left_at_start_stays() {
            let mut s = state_with("abc", 0);

            s.move_cursor(CursorMove::Left);

            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn move_right() {
            let mut s = state_with("abc", 1);

            s.move_cursor(CursorMove::Right);

            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn move_right_at_end_stays() {
            let mut s = state_with("abc", 3);

            s.move_cursor(CursorMove::Right);

            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn move_home() {
            let mut s = state_with("abc", 2);

            s.move_cursor(CursorMove::Home);

            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn move_end() {
            let mut s = state_with("abc", 0);

            s.move_cursor(CursorMove::End);

            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn move_up_is_noop() {
            let mut s = state_with("abc", 1);

            s.move_cursor(CursorMove::Up);

            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn move_down_is_noop() {
            let mut s = state_with("abc", 1);

            s.move_cursor(CursorMove::Down);

            assert_eq!(s.cursor(), 1);
        }
    }

    mod viewport_tests {
        use super::*;

        #[test]
        fn cursor_within_viewport_no_change() {
            let mut s = TextInputState::with_viewport("abcdef", 2, 0);

            s.update_viewport(5);

            assert_eq!(s.viewport_offset(), 0);
        }

        #[test]
        fn cursor_past_right_edge_scrolls() {
            let mut s = TextInputState::with_viewport("abcdefgh", 7, 0);

            s.update_viewport(5);

            assert_eq!(s.viewport_offset(), 3);
        }

        #[test]
        fn cursor_before_viewport_scrolls_left() {
            let mut s = TextInputState::with_viewport("abcdefgh", 1, 4);

            s.update_viewport(5);

            assert_eq!(s.viewport_offset(), 1);
        }

        #[test]
        fn cursor_at_end_reserves_space_for_block_cursor() {
            let mut s = TextInputState::with_viewport("abcde", 5, 0);

            s.update_viewport(5);

            // cursor == char_count, effective_width = 5 - 1 = 4
            // cursor(5) >= viewport(0) + effective(4), so scroll
            assert_eq!(s.viewport_offset(), 2);
        }

        #[test]
        fn zero_visible_width() {
            let mut s = TextInputState::with_viewport("abc", 1, 2);

            s.update_viewport(0);

            assert_eq!(s.viewport_offset(), 0);
        }

        #[test]
        fn cursor_on_last_char_no_extra_reserve() {
            let mut s = TextInputState::with_viewport("abcde", 4, 0);

            s.update_viewport(5);

            // cursor(4) is on last char (not at end), effective_width = 5
            // cursor(4) < viewport(0) + effective(5), no scroll needed
            assert_eq!(s.viewport_offset(), 0);
        }
    }

    mod set_content_and_clear {
        use super::*;

        #[test]
        fn set_content_sets_cursor_to_end() {
            let mut s = TextInputState::default();

            s.set_content("hello".to_string());

            assert_eq!(s.content(), "hello");
            assert_eq!(s.cursor(), 5);
            assert_eq!(s.viewport_offset(), 0);
        }

        #[test]
        fn set_content_resets_viewport() {
            let mut s = TextInputState::with_viewport("old", 2, 5);

            s.set_content("new value".to_string());

            assert_eq!(s.viewport_offset(), 0);
        }

        #[test]
        fn set_content_multibyte() {
            let mut s = TextInputState::default();

            s.set_content("日本語".to_string());

            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn clear_resets_all() {
            let mut s = TextInputState::with_viewport("hello", 3, 2);

            s.clear();

            assert_eq!(s.content(), "");
            assert_eq!(s.cursor(), 0);
            assert_eq!(s.viewport_offset(), 0);
        }
    }

    mod char_count_tests {
        use super::*;

        #[test]
        fn ascii_string() {
            let s = state_with("hello", 0);

            assert_eq!(s.char_count(), 5);
        }

        #[test]
        fn multibyte_string() {
            let s = state_with("日本語", 0);

            assert_eq!(s.char_count(), 3);
        }

        #[test]
        fn empty_string() {
            let s = TextInputState::default();

            assert_eq!(s.char_count(), 0);
        }

        #[test]
        fn mixed_ascii_and_multibyte() {
            let s = state_with("a日b本c", 0);

            assert_eq!(s.char_count(), 5);
        }
    }

    mod constructor_tests {
        use super::*;

        #[test]
        fn new_clamps_cursor_to_char_count() {
            let s = TextInputState::new("abc", 100);

            assert_eq!(s.cursor(), 3);
            assert_eq!(s.viewport_offset(), 0);
        }

        #[test]
        fn new_accepts_valid_cursor() {
            let s = TextInputState::new("abc", 1);

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn with_viewport_clamps_cursor() {
            let s = TextInputState::with_viewport("ab", 10, 5);

            assert_eq!(s.cursor(), 2);
            assert_eq!(s.viewport_offset(), 5);
        }

        #[test]
        fn set_cursor_clamps_and_resets_viewport() {
            let mut s = TextInputState::with_viewport("abcde", 3, 2);

            s.set_cursor(100);

            assert_eq!(s.cursor(), 5);
            assert_eq!(s.viewport_offset(), 0);
        }

        #[test]
        fn set_cursor_valid_position() {
            let mut s = TextInputState::with_viewport("abcde", 4, 2);

            s.set_cursor(1);

            assert_eq!(s.cursor(), 1);
            assert_eq!(s.viewport_offset(), 0);
        }
    }
}
