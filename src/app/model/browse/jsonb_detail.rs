use crate::app::model::shared::multi_line_input::MultiLineInputState;
use crate::app::model::shared::text_input::TextInputState;
use crate::app::policy::json::visible_line_indices;

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
    pretty_original: String,
    mode: JsonbDetailMode,
    tree: JsonTree,
    visible_indices: Vec<usize>,
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
        let visible_indices = visible_line_indices(&tree);
        let pretty_original = serde_json::from_str::<serde_json::Value>(&original_json)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| original_json.clone());
        Self {
            row,
            col,
            column_name,
            original_json,
            pretty_original,
            mode: JsonbDetailMode::Viewing,
            tree,
            visible_indices,
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

    pub fn pretty_original(&self) -> &str {
        &self.pretty_original
    }

    pub fn tree(&self) -> &JsonTree {
        &self.tree
    }

    pub fn visible_indices(&self) -> &[usize] {
        &self.visible_indices
    }

    pub fn visible_count(&self) -> usize {
        self.visible_indices.len()
    }

    fn rebuild_visible_indices(&mut self) {
        self.visible_indices = visible_line_indices(&self.tree);
    }

    pub fn toggle_fold(&mut self, visible_line_idx: usize) {
        if let Some(&real_idx) = self.visible_indices.get(visible_line_idx) {
            self.tree.toggle_fold(real_idx);
            self.rebuild_visible_indices();
        }
    }

    pub fn fold_all(&mut self) {
        self.tree.fold_all();
        self.rebuild_visible_indices();
        let vc = self.visible_count();
        self.clamp_cursor(vc);
        self.clamp_scroll(vc);
    }

    pub fn unfold_all(&mut self) {
        self.tree.unfold_all();
        self.rebuild_visible_indices();
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn set_scroll_offset(&mut self, scroll_offset: usize) {
        self.scroll_offset = scroll_offset;
    }

    pub fn adjusted_scroll(&self, viewport_height: usize) -> usize {
        let visible_count = self.visible_indices.len();
        let selected_line = self.selected_line.min(visible_count.saturating_sub(1));
        let scroll_offset = self.scroll_offset.min(visible_count.saturating_sub(1));

        if viewport_height == 0 || visible_count == 0 {
            return scroll_offset;
        }

        let max_scroll = visible_count.saturating_sub(viewport_height);
        let candidate = if selected_line < scroll_offset {
            selected_line
        } else if selected_line >= scroll_offset + viewport_height {
            selected_line - viewport_height + 1
        } else {
            scroll_offset
        };

        candidate.min(max_scroll)
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

    pub fn set_selected_line(&mut self, line: usize) {
        self.selected_line = line;
    }

    pub fn cursor_up(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        self.selected_line = self.selected_line.saturating_sub(1);
    }

    pub fn cursor_down(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        let max = visible_count.saturating_sub(1);
        if self.selected_line < max {
            self.selected_line += 1;
        }
    }

    pub fn cursor_to_top(&mut self) {
        self.selected_line = 0;
        self.scroll_offset = 0;
    }

    pub fn cursor_to_end(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        self.selected_line = visible_count.saturating_sub(1);
    }

    pub fn clamp_cursor(&mut self, visible_count: usize) {
        if visible_count == 0 {
            self.selected_line = 0;
        } else if self.selected_line >= visible_count {
            self.selected_line = visible_count - 1;
        }
    }

    pub fn clamp_scroll(&mut self, visible_count: usize) {
        if visible_count == 0 {
            self.scroll_offset = 0;
        } else if self.scroll_offset >= visible_count {
            self.scroll_offset = visible_count.saturating_sub(1);
        }
    }

    pub fn enter_search(&mut self) {
        self.mode = JsonbDetailMode::Searching;
        self.search.active = true;
        self.search.input.set_content(String::new());
        self.search.matches.clear();
        self.search.current_match = 0;
    }

    pub fn exit_search(&mut self) {
        self.search.active = false;
        self.mode = JsonbDetailMode::Viewing;
    }

    pub fn enter_edit(&mut self, pretty_json: String, target_line: usize) {
        let cursor = char_offset_of_line(&pretty_json, target_line);
        self.editor = MultiLineInputState::new(pretty_json, cursor);
        self.validation_error = None;
        self.mode = JsonbDetailMode::Editing;
    }

    pub fn exit_edit(&mut self) {
        self.mode = JsonbDetailMode::Viewing;
    }

    pub fn replace_tree(&mut self, tree: JsonTree) {
        self.tree = tree;
        self.rebuild_visible_indices();
        self.selected_line = 0;
        self.scroll_offset = 0;
    }

    pub fn current_json_for_yank(&self) -> String {
        if self.has_pending_changes() {
            // Return compact JSON from editor content
            serde_json::from_str::<serde_json::Value>(self.editor.content())
                .ok()
                .and_then(|v| serde_json::to_string(&v).ok())
                .unwrap_or_else(|| self.original_json.clone())
        } else {
            self.original_json.clone()
        }
    }

    pub fn has_pending_changes(&self) -> bool {
        let content = self.editor.content();
        if content.is_empty() {
            return false;
        }
        let trimmed = content.trim();
        trimmed != self.original_json.trim() && trimmed != self.pretty_original.trim()
    }

    pub fn validate_editor_content(&mut self) -> Result<String, String> {
        let content = self.editor.content().to_string();
        match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(_) => {
                self.validation_error = None;
                Ok(content)
            }
            Err(e) => {
                let msg = format!("Invalid JSON: {e}");
                self.validation_error = Some(msg.clone());
                Err(msg)
            }
        }
    }
}

fn char_offset_of_line(s: &str, target_line: usize) -> usize {
    let mut line = 0;
    for (i, ch) in s.chars().enumerate() {
        if line == target_line {
            return i;
        }
        if ch == '\n' {
            line += 1;
        }
    }
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::JsonbDetailState;
    use crate::app::model::browse::json_tree::{JsonTree, LineType, TreeLine, TreeValue};

    fn state_with_lines(lines: usize) -> JsonbDetailState {
        let mut state = JsonbDetailState::open(
            0,
            0,
            "col".to_string(),
            "[]".to_string(),
            JsonTree::new(
                (0..lines)
                    .map(|i| TreeLine {
                        depth: 0,
                        key: Some(format!("k{i}")),
                        value: TreeValue::String(format!("v{i}")),
                        collapsed: false,
                        line_type: LineType::KeyValue,
                    })
                    .collect(),
            ),
        );
        state.set_scroll_offset(0);
        state
    }

    #[test]
    fn adjusted_scroll_keeps_offset_when_viewport_height_is_zero() {
        let mut state = state_with_lines(10);
        state.set_selected_line(4);
        state.set_scroll_offset(3);

        assert_eq!(state.adjusted_scroll(0), 3);
    }

    #[test]
    fn adjusted_scroll_moves_up_when_selection_is_above_scroll() {
        let mut state = state_with_lines(10);
        state.set_selected_line(2);
        state.set_scroll_offset(5);

        assert_eq!(state.adjusted_scroll(4), 2);
    }

    #[test]
    fn adjusted_scroll_moves_down_when_selection_is_below_viewport() {
        let mut state = state_with_lines(10);
        state.set_selected_line(8);
        state.set_scroll_offset(3);

        assert_eq!(state.adjusted_scroll(4), 5);
    }

    #[test]
    fn adjusted_scroll_keeps_offset_when_selection_is_visible() {
        let mut state = state_with_lines(10);
        state.set_selected_line(4);
        state.set_scroll_offset(3);

        assert_eq!(state.adjusted_scroll(4), 3);
    }

    #[test]
    fn adjusted_scroll_clamps_to_visible_count() {
        let mut state = state_with_lines(3);
        state.set_selected_line(10);
        state.set_scroll_offset(9);

        assert_eq!(state.adjusted_scroll(5), 0);
    }
}
