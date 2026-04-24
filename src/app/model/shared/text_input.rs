use crate::update::action::CursorMove;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInputState {
    content: String,
    char_count: usize,
    cursor: usize,
    viewport_offset: usize,
}

pub trait TextInputLike {
    fn text_input(&self) -> &TextInputState;

    fn content(&self) -> &str {
        self.text_input().content()
    }

    fn cursor(&self) -> usize {
        self.text_input().cursor()
    }

    fn char_count(&self) -> usize {
        self.text_input().char_count()
    }
}

impl TextInputState {
    pub fn new(content: impl Into<String>, cursor: usize) -> Self {
        let content = content.into();
        let char_count = content.chars().count();
        Self {
            content,
            char_count,
            cursor: cursor.min(char_count),
            viewport_offset: 0,
        }
    }

    pub fn with_viewport(
        content: impl Into<String>,
        cursor: usize,
        viewport_offset: usize,
    ) -> Self {
        let content = content.into();
        let char_count = content.chars().count();
        Self {
            content,
            char_count,
            cursor: cursor.min(char_count),
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
        self.cursor = pos.min(self.char_count);
        self.viewport_offset = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        let byte_idx = char_to_byte_index(&self.content, self.cursor);
        self.content.insert(byte_idx, c);
        self.cursor += 1;
        self.char_count += 1;
    }

    pub fn insert_str(&mut self, text: &str) {
        let byte_idx = char_to_byte_index(&self.content, self.cursor);
        let inserted_chars = text.chars().count();
        self.content.insert_str(byte_idx, text);
        self.cursor += inserted_chars;
        self.char_count += inserted_chars;
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
        self.char_count -= 1;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.char_count {
            return;
        }
        let start = char_to_byte_index(&self.content, self.cursor);
        let end = char_to_byte_index(&self.content, self.cursor + 1);
        self.content.drain(start..end);
        self.char_count -= 1;
    }

    pub fn move_cursor(&mut self, movement: CursorMove) {
        match movement {
            CursorMove::Left => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            CursorMove::Right => {
                if self.cursor < self.char_count {
                    self.cursor += 1;
                }
            }
            CursorMove::Home
            | CursorMove::LineStart
            | CursorMove::BufferStart
            | CursorMove::FirstLine => {
                self.cursor = 0;
            }
            CursorMove::End
            | CursorMove::LineEnd
            | CursorMove::BufferEnd
            | CursorMove::LastLine => {
                self.cursor = self.char_count;
            }
            CursorMove::WordForward => {
                self.cursor = next_word_start(&self.content, self.cursor);
            }
            CursorMove::WordBackward => {
                self.cursor = previous_word_start(&self.content, self.cursor);
            }
            CursorMove::Up
            | CursorMove::Down
            | CursorMove::ViewportTop
            | CursorMove::ViewportMiddle
            | CursorMove::ViewportBottom => {}
        }
    }

    pub fn update_viewport(&mut self, visible_width: usize) {
        if visible_width == 0 {
            self.viewport_offset = 0;
            return;
        }

        // █ occupies one terminal cell at end-of-input; shrink effective width to keep it visible
        let effective_width = if self.cursor == self.char_count {
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
        let char_count = s.chars().count();
        self.content = s;
        self.char_count = char_count;
        self.cursor = char_count;
        self.viewport_offset = 0;
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.char_count = 0;
        self.cursor = 0;
        self.viewport_offset = 0;
    }

    pub fn char_count(&self) -> usize {
        self.char_count
    }
}

impl TextInputLike for TextInputState {
    fn text_input(&self) -> &TextInputState {
        self
    }
}

fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

#[cfg(test)]
mod perf_tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordKind {
    Keyword,
    Symbol,
}

fn classify_word_char(ch: char) -> Option<WordKind> {
    if ch.is_whitespace() {
        None
    } else if ch.is_alphanumeric() || ch == '_' {
        Some(WordKind::Keyword)
    } else {
        Some(WordKind::Symbol)
    }
}

pub(super) fn next_word_start(content: &str, cursor: usize) -> usize {
    let char_count = content.chars().count();
    if cursor >= char_count {
        return char_count;
    }

    let mut chars = content.chars().enumerate().skip(cursor);
    let Some((_, ch)) = chars.next() else {
        return char_count;
    };
    let mut idx = cursor;

    if ch.is_whitespace() {
        idx += 1;
        for (_, ch) in chars {
            if ch.is_whitespace() {
                idx += 1;
            } else {
                break;
            }
        }
        return idx;
    }

    let kind = classify_word_char(ch).expect("non-whitespace char has a kind");
    idx += 1;
    for (_, ch) in chars {
        match classify_word_char(ch) {
            Some(current) if current == kind => idx += 1,
            Some(_) => break,
            None => {
                idx += 1;
                for (_, ch) in content.chars().enumerate().skip(idx) {
                    if ch.is_whitespace() {
                        idx += 1;
                    } else {
                        break;
                    }
                }
                break;
            }
        }
    }

    idx
}

pub(super) fn previous_word_start(content: &str, cursor: usize) -> usize {
    if cursor == 0 || content.is_empty() {
        return 0;
    }

    let target = cursor.saturating_sub(1);
    let mut current_run_start = 0;
    let mut current_run_kind = None;
    let mut last_non_whitespace_run_start = None;

    for (idx, ch) in content.chars().enumerate() {
        if idx > target {
            break;
        }
        if ch.is_whitespace() {
            current_run_kind = None;
            continue;
        }

        let kind = classify_word_char(ch).expect("non-whitespace char has a kind");
        if current_run_kind != Some(kind) {
            current_run_start = idx;
            current_run_kind = Some(kind);
        }

        last_non_whitespace_run_start = Some(current_run_start);
    }

    last_non_whitespace_run_start.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

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
        fn inserts_at_beginning() {
            let mut s = state_with("cd", 0);

            s.insert_str("ab");

            assert_eq!(s.content(), "abcd");
            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn inserts_at_middle() {
            let mut s = state_with("ad", 1);

            s.insert_str("bc");

            assert_eq!(s.content(), "abcd");
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn inserts_multibyte() {
            let mut s = state_with("あえ", 1);

            s.insert_str("いう");

            assert_eq!(s.content(), "あいうえ");
            assert_eq!(s.cursor(), 3);
        }
    }

    mod backspace_tests {
        use super::*;

        #[test]
        fn at_start_is_noop() {
            let mut s = state_with("abc", 0);

            s.backspace();

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn at_end() {
            let mut s = state_with("abc", 3);

            s.backspace();

            assert_eq!(s.content(), "ab");
            assert_eq!(s.cursor(), 2);
        }

        #[test]
        fn at_middle() {
            let mut s = state_with("abc", 2);

            s.backspace();

            assert_eq!(s.content(), "ac");
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn on_empty_string() {
            let mut s = TextInputState::default();

            s.backspace();

            assert_eq!(s.content(), "");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn multibyte() {
            let mut s = state_with("あいう", 2);

            s.backspace();

            assert_eq!(s.content(), "あう");
            assert_eq!(s.cursor(), 1);
        }
    }

    mod delete_tests {
        use super::*;

        #[test]
        fn at_end_is_noop() {
            let mut s = state_with("abc", 3);

            s.delete();

            assert_eq!(s.content(), "abc");
            assert_eq!(s.cursor(), 3);
        }

        #[test]
        fn at_beginning() {
            let mut s = state_with("abc", 0);

            s.delete();

            assert_eq!(s.content(), "bc");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn at_middle() {
            let mut s = state_with("abc", 1);

            s.delete();

            assert_eq!(s.content(), "ac");
            assert_eq!(s.cursor(), 1);
        }

        #[test]
        fn on_empty_string() {
            let mut s = TextInputState::default();

            s.delete();

            assert_eq!(s.content(), "");
            assert_eq!(s.cursor(), 0);
        }

        #[test]
        fn multibyte() {
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
        fn move_last_line_moves_to_end() {
            let mut s = state_with("abc", 0);

            s.move_cursor(CursorMove::LastLine);

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

        #[rstest]
        #[case("foo bar", 0, CursorMove::WordForward, 4)]
        #[case("foo.bar", 0, CursorMove::WordForward, 3)]
        #[case("  foo", 0, CursorMove::WordForward, 2)]
        #[case("foo  ", 0, CursorMove::WordForward, 5)]
        #[case("あいう えお", 0, CursorMove::WordForward, 4)]
        fn word_forward_boundaries(
            #[case] content: &str,
            #[case] cursor: usize,
            #[case] movement: CursorMove,
            #[case] expected: usize,
        ) {
            let mut s = state_with(content, cursor);

            s.move_cursor(movement);

            assert_eq!(s.cursor(), expected);
        }

        #[rstest]
        #[case("foo bar", 7, CursorMove::WordBackward, 4)]
        #[case("foo.bar", 7, CursorMove::WordBackward, 4)]
        #[case("  foo", 4, CursorMove::WordBackward, 2)]
        #[case("foo  ", 5, CursorMove::WordBackward, 0)]
        #[case("あいう えお", 6, CursorMove::WordBackward, 4)]
        fn word_backward_boundaries(
            #[case] content: &str,
            #[case] cursor: usize,
            #[case] movement: CursorMove,
            #[case] expected: usize,
        ) {
            let mut s = state_with(content, cursor);

            s.move_cursor(movement);

            assert_eq!(s.cursor(), expected);
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
        fn empty_string_has_zero_chars() {
            let s = TextInputState::default();

            assert_eq!(s.char_count(), 0);
        }

        #[test]
        fn mixed_ascii_and_multibyte() {
            let s = state_with("a日b本c", 0);

            assert_eq!(s.char_count(), 5);
        }

        #[test]
        fn mutations_keep_cached_char_count_in_sync() {
            let mut s = state_with("ab", 2);

            s.insert_char('日');
            assert_eq!(s.char_count(), 3);

            s.insert_str("本語");
            assert_eq!(s.char_count(), 5);

            s.backspace();
            assert_eq!(s.char_count(), 4);

            s.set_cursor(1);
            s.delete();
            assert_eq!(s.char_count(), 3);

            s.set_content("xyz".to_string());
            assert_eq!(s.char_count(), 3);

            s.clear();
            assert_eq!(s.char_count(), 0);
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
