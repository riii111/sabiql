use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct RowJsonState {
    row: usize,
    content: String,
    scroll_offset: usize,
    active: bool,
}

impl RowJsonState {
    pub fn open(row: usize, columns: &[String], cells: &[String]) -> Self {
        let mut obj = serde_json::Map::new();
        for (col, cell) in columns.iter().zip(cells.iter()) {
            obj.insert(col.clone(), infer_json_value(cell));
        }

        let content =
            serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_else(|_| "{}".to_string());

        Self {
            row,
            content,
            scroll_offset: 0,
            active: true,
        }
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn row(&self) -> usize {
        self.row
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_offset_mut(&mut self) -> &mut usize {
        &mut self.scroll_offset
    }

    pub fn line_count(&self) -> usize {
        self.content.lines().count().max(1)
    }

    pub fn content_for_yank(&self) -> String {
        self.content.clone()
    }
}

fn infer_json_value(cell: &str) -> Value {
    if cell.is_empty() {
        return Value::Null;
    }
    if let Ok(value) = serde_json::from_str::<Value>(cell) {
        return value;
    }
    Value::String(cell.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cell_becomes_null() {
        let state = RowJsonState::open(0, &["name".to_string()], &["".to_string()]);

        assert!(state.content().contains("\"name\": null"));
    }

    #[test]
    fn number_string_becomes_number() {
        let state = RowJsonState::open(0, &["count".to_string()], &["42".to_string()]);

        assert!(state.content().contains("\"count\": 42"));
    }

    #[test]
    fn boolean_string_becomes_boolean() {
        let state = RowJsonState::open(0, &["active".to_string()], &["true".to_string()]);

        assert!(state.content().contains("\"active\": true"));
    }

    #[test]
    fn plain_text_stays_string() {
        let state = RowJsonState::open(0, &["title".to_string()], &["hello world".to_string()]);

        assert!(state.content().contains("\"title\": \"hello world\""));
    }

    #[test]
    fn nested_json_object_is_embedded() {
        let state = RowJsonState::open(
            0,
            &["settings".to_string()],
            &[r#"{"theme":"dark"}"#.to_string()],
        );

        assert!(
            state
                .content()
                .contains("\"settings\": {\n  \"theme\": \"dark\"\n}")
        );
    }

    #[test]
    fn multiple_columns_build_object() {
        let state = RowJsonState::open(
            0,
            &["id".to_string(), "name".to_string()],
            &["1".to_string(), "alice".to_string()],
        );

        assert!(state.content().contains("\"id\": 1"));
        assert!(state.content().contains("\"name\": \"alice\""));
    }

    #[test]
    fn row_index_is_preserved() {
        let state = RowJsonState::open(7, &["id".to_string()], &["1".to_string()]);

        assert_eq!(state.row(), 7);
    }
}
