use crate::model::shared::scroll::ScrollOffset;
use unicode_width::UnicodeWidthChar;

use crate::model::shared::text_input::TextInputState;
use crate::update::action::{ScrollAmount, ScrollDirection};

const DEFAULT_VIEWPORT_WIDTH: usize = 80;
const DEFAULT_VISIBLE_ROWS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct DetailSearchState {
    input: TextInputState,
    matches: Vec<usize>,
    current_match: usize,
    active: bool,
}

impl DetailSearchState {
    pub fn input(&self) -> &TextInputState {
        &self.input
    }

    pub fn input_mut(&mut self) -> &mut TextInputState {
        &mut self.input
    }

    pub fn matches(&self) -> &[usize] {
        &self.matches
    }

    pub fn current_match(&self) -> usize {
        self.current_match
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_matches(&mut self, matches: Vec<usize>) {
        self.matches = matches;
        self.current_match = 0;
    }

    pub fn advance_to_next_match(&mut self) {
        if !self.matches.is_empty() {
            self.current_match = (self.current_match + 1) % self.matches.len();
        }
    }

    pub fn advance_to_prev_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match = if self.current_match == 0 {
            self.matches.len() - 1
        } else {
            self.current_match - 1
        };
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.matches.clear();
        self.current_match = 0;
    }

    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailDisplayMode {
    #[default]
    RawText,
    FormattedJson,
}

#[derive(Debug, Clone)]
pub struct DetailContentState {
    row: usize,
    col: usize,
    column_name: String,
    original_content: String,
    display_content: String,
    display_mode: DetailDisplayMode,
}

impl DetailContentState {
    pub fn new(
        row: usize,
        col: usize,
        column_name: String,
        original_content: String,
        display_content: String,
    ) -> Self {
        Self::new_with_display_mode(
            row,
            col,
            column_name,
            original_content,
            display_content,
            DetailDisplayMode::RawText,
        )
    }

    pub fn new_with_display_mode(
        row: usize,
        col: usize,
        column_name: String,
        original_content: String,
        display_content: String,
        display_mode: DetailDisplayMode,
    ) -> Self {
        Self {
            row,
            col,
            column_name,
            original_content,
            display_content,
            display_mode,
        }
    }

    pub fn row(&self) -> usize {
        self.row
    }

    pub fn col(&self) -> usize {
        self.col
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn content(&self) -> &str {
        &self.display_content
    }

    pub fn original_content(&self) -> &str {
        &self.original_content
    }

    pub fn display_mode(&self) -> DetailDisplayMode {
        self.display_mode
    }
}

impl Default for DetailContentState {
    fn default() -> Self {
        Self::new(0, 0, String::new(), String::new(), String::new())
    }
}

#[derive(Debug, Clone)]
pub struct ReadOnlyDetailState {
    content: DetailContentState,
    scroll_offset: ScrollOffset,
    visible_rows: usize,
    viewport_width: usize,
    search: DetailSearchState,
    active: bool,
}

impl Default for ReadOnlyDetailState {
    fn default() -> Self {
        Self {
            content: DetailContentState::default(),
            scroll_offset: ScrollOffset::default(),
            visible_rows: DEFAULT_VISIBLE_ROWS,
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
            search: DetailSearchState::default(),
            active: false,
        }
    }
}

impl ReadOnlyDetailState {
    pub fn open(
        row: usize,
        col: usize,
        column_name: String,
        original_content: String,
        display_content: String,
    ) -> Self {
        Self::open_with_display_mode(
            row,
            col,
            column_name,
            original_content,
            display_content,
            DetailDisplayMode::RawText,
        )
    }

    pub fn open_with_display_mode(
        row: usize,
        col: usize,
        column_name: String,
        original_content: String,
        display_content: String,
        display_mode: DetailDisplayMode,
    ) -> Self {
        Self {
            content: DetailContentState::new_with_display_mode(
                row,
                col,
                column_name,
                original_content,
                display_content,
                display_mode,
            ),
            active: true,
            ..Self::default()
        }
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn row(&self) -> usize {
        self.content.row()
    }

    pub fn col(&self) -> usize {
        self.content.col()
    }

    pub fn column_name(&self) -> &str {
        self.content.column_name()
    }

    pub fn content(&self) -> &str {
        self.content.content()
    }

    pub fn original_content(&self) -> &str {
        self.content.original_content()
    }

    pub fn display_mode(&self) -> DetailDisplayMode {
        self.content.display_mode()
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset.get()
    }

    pub fn search(&self) -> &DetailSearchState {
        &self.search
    }

    pub fn search_mut(&mut self) -> &mut DetailSearchState {
        &mut self.search
    }

    pub fn set_viewport_metrics(&mut self, visible_rows: usize, viewport_width: usize) {
        self.visible_rows = visible_rows.max(1);
        self.viewport_width = viewport_width.max(1);
        self.clamp_scroll();
    }

    pub fn enter_search(&mut self) {
        self.search.reset();
        self.search.activate();
    }

    pub fn exit_search(&mut self) {
        self.search.deactivate();
    }

    pub fn scroll(&mut self, direction: ScrollDirection, amount: ScrollAmount) {
        let delta = match amount {
            ScrollAmount::Line => 1,
            ScrollAmount::HalfPage => (self.visible_rows / 2).max(1),
            ScrollAmount::FullPage => self.visible_rows,
            ScrollAmount::ToStart => {
                self.scroll_offset.set(0);
                return;
            }
            ScrollAmount::ToEnd => {
                self.scroll_offset.set(self.max_scroll_offset());
                return;
            }
            _ => return,
        };

        let max = self.max_scroll_offset();
        self.scroll_offset.scroll(direction, delta, max);
    }

    pub fn scroll_to_match(&mut self) {
        let Some(&match_pos) = self.search.matches.get(self.search.current_match) else {
            return;
        };
        let offset =
            visual_row_for_char_offset(self.content.content(), match_pos, self.viewport_width)
                .min(self.max_scroll_offset());
        self.scroll_offset.set(offset);
    }

    fn clamp_scroll(&mut self) {
        let max = self.max_scroll_offset();
        self.scroll_offset.clamp(max);
    }

    fn max_scroll_offset(&self) -> usize {
        visual_line_count(self.content.content(), self.viewport_width)
            .saturating_sub(self.visible_rows)
    }
}

fn visual_line_count(content: &str, width: usize) -> usize {
    if width == 0 {
        return content.split('\n').count();
    }
    visual_row_for_char_offset(content, usize::MAX, width).saturating_add(1)
}

fn visual_row_for_char_offset(content: &str, target: usize, width: usize) -> usize {
    if width == 0 {
        return 0;
    }

    let mut visual_row = 0;
    let mut current_width = 0;
    for (char_offset, ch) in content.chars().enumerate() {
        if char_offset >= target {
            break;
        }
        if ch == '\n' {
            visual_row += 1;
            current_width = 0;
            continue;
        }
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width > 0 && current_width + ch_width > width {
            visual_row += 1;
            current_width = 0;
        }
        current_width += ch_width;
    }
    visual_row
}

#[cfg(test)]
mod tests {
    use super::*;

    mod scrolling {
        use super::*;

        #[test]
        fn long_single_line_uses_wrapped_visual_rows_for_max_scroll() {
            let content = "a".repeat(100);
            let mut state =
                ReadOnlyDetailState::open(0, 0, "body".to_string(), content.clone(), content);
            state.set_viewport_metrics(3, 10);

            state.scroll(ScrollDirection::Down, ScrollAmount::ToEnd);

            assert_eq!(state.scroll_offset(), 7);
        }

        #[test]
        fn end_scroll_counts_wrapped_and_empty_lines() {
            let content = "ab界c\n\nx".to_string();
            let mut state =
                ReadOnlyDetailState::open(0, 0, "body".to_string(), content.clone(), content);
            state.set_viewport_metrics(1, 3);
            state.scroll(ScrollDirection::Down, ScrollAmount::ToEnd);
            assert_eq!(state.scroll_offset(), 3);
        }

        #[test]
        fn search_match_scrolls_to_wrapped_visual_row() {
            let content = format!("{}needle", "a".repeat(35));
            let mut state =
                ReadOnlyDetailState::open(0, 0, "body".to_string(), content.clone(), content);
            state.set_viewport_metrics(3, 10);
            state.search_mut().set_matches(vec![35]);

            state.scroll_to_match();

            assert_eq!(state.scroll_offset(), 2);
        }
    }
}
