use crate::app::model::shared::multi_line_input::MultiLineInputState;
use crate::app::model::shared::text_input::TextInputState;

use super::json_tree::JsonTree;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonbDetailMode {
    #[default]
    Viewing,
    Editing,
    Searching,
}

#[derive(Debug, Clone, Default)]
pub struct JsonbSearchState {
    pub input: TextInputState,
    pub matches: Vec<usize>,
    pub current_match: usize,
    pub active: bool,
}

#[derive(Debug, Clone, Default)]
pub struct JsonbDetailState {
    row: usize,
    col: usize,
    column_name: String,
    original_json: String,
    mode: JsonbDetailMode,
    tree: JsonTree,
    scroll_offset: usize,
    selected_line: usize,
    editor: MultiLineInputState,
    validation_error: Option<String>,
    search: JsonbSearchState,
    active: bool,
}

impl JsonbDetailState {
    pub fn open(
        row: usize,
        col: usize,
        column_name: String,
        original_json: String,
        tree: JsonTree,
    ) -> Self {
        Self {
            row,
            col,
            column_name,
            original_json,
            mode: JsonbDetailMode::Viewing,
            tree,
            scroll_offset: 0,
            selected_line: 0,
            editor: MultiLineInputState::default(),
            validation_error: None,
            search: JsonbSearchState::default(),
            active: true,
        }
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn mode(&self) -> JsonbDetailMode {
        self.mode
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

    pub fn original_json(&self) -> &str {
        &self.original_json
    }

    pub fn tree(&self) -> &JsonTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut JsonTree {
        &mut self.tree
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn selected_line(&self) -> usize {
        self.selected_line
    }

    pub fn editor(&self) -> &MultiLineInputState {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut MultiLineInputState {
        &mut self.editor
    }

    pub fn validation_error(&self) -> Option<&str> {
        self.validation_error.as_deref()
    }

    pub fn set_validation_error(&mut self, error: Option<String>) {
        self.validation_error = error;
    }

    pub fn search(&self) -> &JsonbSearchState {
        &self.search
    }

    pub fn search_mut(&mut self) -> &mut JsonbSearchState {
        &mut self.search
    }

    pub fn set_mode(&mut self, mode: JsonbDetailMode) {
        self.mode = mode;
    }

    /// Move cursor up in visible lines, clamping at 0.
    pub fn cursor_up(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        self.selected_line = self.selected_line.saturating_sub(1);
    }

    /// Move cursor down in visible lines, clamping at last visible line.
    pub fn cursor_down(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        let max = visible_count.saturating_sub(1);
        if self.selected_line < max {
            self.selected_line += 1;
        }
    }

    /// Jump to first visible line.
    pub fn cursor_to_top(&mut self) {
        self.selected_line = 0;
        self.scroll_offset = 0;
    }

    /// Jump to last visible line.
    pub fn cursor_to_end(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        self.selected_line = visible_count.saturating_sub(1);
    }

    /// Ensure the selected line is visible within the viewport.
    pub fn adjust_scroll(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.selected_line < self.scroll_offset {
            self.scroll_offset = self.selected_line;
        } else if self.selected_line >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected_line - viewport_height + 1;
        }
    }

    /// Clamp selected_line after fold/unfold changes the visible line count.
    pub fn clamp_cursor(&mut self, visible_count: usize) {
        if visible_count == 0 {
            self.selected_line = 0;
        } else if self.selected_line >= visible_count {
            self.selected_line = visible_count - 1;
        }
    }
}
