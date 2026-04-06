use crate::app::model::shared::multi_line_input::MultiLineInputState;
use crate::app::model::shared::text_input::{TextInputLike, TextInputState};

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
    scroll_offset: usize,
    editor: MultiLineInputState,
    validation_error: Option<String>,
    search: JsonbSearchState,
    active: bool,
}

impl JsonbDetailState {
    pub fn open(row: usize, col: usize, column_name: String, original_json: String) -> Self {
        let pretty_original = serde_json::from_str::<serde_json::Value>(&original_json)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| original_json.clone());
        Self {
            row,
            col,
            column_name,
            original_json,
            editor: MultiLineInputState::new(pretty_original.clone(), 0),
            pretty_original,
            mode: JsonbDetailMode::Viewing,
            scroll_offset: 0,
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

    pub fn set_scroll_offset(&mut self, scroll_offset: usize) {
        self.scroll_offset = scroll_offset;
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

    pub fn enter_edit(&mut self) {
        self.validation_error = None;
        self.mode = JsonbDetailMode::Editing;
    }

    pub fn exit_edit(&mut self) {
        self.mode = JsonbDetailMode::Viewing;
    }

    pub fn current_json_for_yank(&self) -> String {
        if self.has_pending_changes() {
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

#[cfg(test)]
mod tests {
    use super::JsonbDetailState;
    use crate::app::model::shared::text_input::TextInputLike;

    #[test]
    fn open_initializes_editor_with_pretty_json() {
        let state = JsonbDetailState::open(
            0,
            0,
            "settings".to_string(),
            r#"{"theme":"dark","count":5}"#.to_string(),
        );

        assert_eq!(state.editor().cursor(), 0);
        assert_eq!(
            state.editor().content(),
            "{\n  \"count\": 5,\n  \"theme\": \"dark\"\n}"
        );
    }
}
