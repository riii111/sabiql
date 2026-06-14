use crate::model::shared::detail_view::{DetailSearchState, ReadOnlyDetailState};
use crate::model::shared::multi_line_input::MultiLineInputState;
use crate::model::shared::text_input::TextInputLike;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonbDetailMode {
    #[default]
    Viewing,
    Editing,
    Searching,
}

pub type JsonbSearchState = DetailSearchState;

#[derive(Debug, Clone, Default)]
pub struct JsonbDetailState {
    detail: ReadOnlyDetailState,
    mode: JsonbDetailMode,
    editor: MultiLineInputState,
    validation_error: Option<String>,
    pub(crate) active: bool,
}

impl JsonbDetailState {
    #[cfg(test)]
    pub fn set_mode(&mut self, mode: JsonbDetailMode) {
        self.mode = mode;
    }

    pub fn open_pretty(
        row: usize,
        col: usize,
        column_name: String,
        original_json: String,
        pretty_original: String,
    ) -> Self {
        Self {
            detail: ReadOnlyDetailState::open(
                row,
                col,
                column_name,
                original_json,
                pretty_original.clone(),
            ),
            editor: MultiLineInputState::new(pretty_original, 0),
            mode: JsonbDetailMode::Viewing,
            validation_error: None,
            active: true,
        }
    }

    pub fn open(row: usize, col: usize, column_name: String, original_json: String) -> Self {
        let pretty_original = serde_json::from_str::<serde_json::Value>(&original_json)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| original_json.clone());
        Self::open_pretty(row, col, column_name, original_json, pretty_original)
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
        self.detail.row()
    }

    pub fn col(&self) -> usize {
        self.detail.col()
    }

    pub fn column_name(&self) -> &str {
        self.detail.column_name()
    }

    pub fn original_json(&self) -> &str {
        self.detail.original_content()
    }

    pub fn pretty_original(&self) -> &str {
        self.detail.content()
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

    pub fn search(&self) -> &JsonbSearchState {
        self.detail.search()
    }

    pub fn search_mut(&mut self) -> &mut JsonbSearchState {
        self.detail.search_mut()
    }

    pub fn enter_search(&mut self) {
        self.mode = JsonbDetailMode::Searching;
        self.detail.enter_search();
    }

    pub fn exit_search(&mut self) {
        self.detail.exit_search();
        self.mode = JsonbDetailMode::Viewing;
    }

    pub fn enter_edit(&mut self) {
        self.detail.exit_search();
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
                .unwrap_or_else(|| self.original_json().to_string())
        } else {
            self.original_json().to_string()
        }
    }

    pub fn has_pending_changes(&self) -> bool {
        let content = self.editor.content();
        if content.is_empty() {
            return false;
        }
        let trimmed = content.trim();
        trimmed != self.original_json().trim() && trimmed != self.pretty_original().trim()
    }

    pub fn validate_editor_content(&mut self) {
        self.validation_error =
            match serde_json::from_str::<serde_json::Value>(self.editor.content()) {
                Ok(_) => None,
                Err(e) => Some(format!("Invalid JSON: {e}")),
            };
    }
}

#[cfg(test)]
mod tests {
    use super::JsonbDetailState;
    use crate::model::shared::text_input::TextInputLike;

    #[test]
    fn open_prettifies_valid_json_into_editor() {
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

    #[test]
    fn open_pretty_uses_provided_pretty_content() {
        let state = JsonbDetailState::open_pretty(
            0,
            0,
            "settings".to_string(),
            r#"{"theme":"dark","count":5}"#.to_string(),
            "{\n  \"theme\": \"custom\"\n}".to_string(),
        );

        assert_eq!(state.editor().cursor(), 0);
        assert_eq!(state.editor().content(), "{\n  \"theme\": \"custom\"\n}");
    }

    #[test]
    fn open_falls_back_to_original_input_when_json_is_invalid() {
        let state =
            JsonbDetailState::open(0, 0, "settings".to_string(), "{invalid json}".to_string());

        assert_eq!(state.editor().cursor(), 0);
        assert_eq!(state.editor().content(), "{invalid json}");
    }

    #[test]
    fn enter_edit_deactivates_search() {
        let mut state = JsonbDetailState::open(
            0,
            0,
            "settings".to_string(),
            r#"{"theme":"dark","count":5}"#.to_string(),
        );
        state.enter_search();

        state.enter_edit();

        assert!(!state.search().is_active());
    }
}
