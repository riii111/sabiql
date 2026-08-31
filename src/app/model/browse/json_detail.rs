use crate::model::shared::detail_view::{DetailContentState, DetailSearchState};
use crate::model::shared::multi_line_input::MultiLineInputState;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonDetailMode {
    #[default]
    Viewing,
    Editing,
    Searching,
}

#[derive(Debug, Clone, Default)]
pub struct JsonDetailState {
    detail: DetailContentState,
    mode: JsonDetailMode,
    editor: MultiLineInputState,
    search: DetailSearchState,
    validation_error: Option<String>,
    pub(crate) active: bool,
}

impl JsonDetailState {
    pub fn open_pretty(
        row: usize,
        col: usize,
        column_name: String,
        original_json: String,
        pretty_original: String,
    ) -> Self {
        Self {
            detail: DetailContentState::new(
                row,
                col,
                column_name,
                original_json,
                pretty_original.clone(),
            ),
            editor: MultiLineInputState::new(pretty_original, 0),
            search: DetailSearchState::default(),
            mode: JsonDetailMode::Viewing,
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

    pub fn mode(&self) -> JsonDetailMode {
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

    pub fn search(&self) -> &DetailSearchState {
        &self.search
    }

    pub fn search_mut(&mut self) -> &mut DetailSearchState {
        &mut self.search
    }

    pub fn enter_search(&mut self) {
        self.mode = JsonDetailMode::Searching;
        self.search.reset();
        self.search.activate();
    }

    pub fn exit_search(&mut self) {
        self.search.deactivate();
        self.mode = JsonDetailMode::Viewing;
    }

    pub fn enter_edit(&mut self) {
        self.search.deactivate();
        self.validation_error = None;
        self.mode = JsonDetailMode::Editing;
    }

    pub fn exit_edit(&mut self) {
        self.mode = JsonDetailMode::Viewing;
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
mod test_support {
    use super::{JsonDetailMode, JsonDetailState};

    impl JsonDetailState {
        pub(crate) fn set_mode(&mut self, mode: JsonDetailMode) {
            self.mode = mode;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonDetailMode, JsonDetailState};

    #[test]
    fn open_prettifies_valid_json_into_editor() {
        let state = JsonDetailState::open(
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
        let state = JsonDetailState::open_pretty(
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
            JsonDetailState::open(0, 0, "settings".to_string(), "{invalid json}".to_string());

        assert_eq!(state.editor().cursor(), 0);
        assert_eq!(state.editor().content(), "{invalid json}");
    }

    #[test]
    fn enter_edit_deactivates_search() {
        let mut state = JsonDetailState::open(
            0,
            0,
            "settings".to_string(),
            r#"{"theme":"dark","count":5}"#.to_string(),
        );
        state.enter_search();

        state.enter_edit();

        assert!(!state.search().is_active());
        assert_eq!(state.mode(), JsonDetailMode::Editing);
    }
}
