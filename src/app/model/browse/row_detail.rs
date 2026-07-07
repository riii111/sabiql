use serde_json::Value;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Default)]
pub struct RowDetailState {
    display_text: String,
    json_text: String,
    scroll_offset: usize,
    horizontal_offset: usize,
    active: bool,
}

impl RowDetailState {
    pub fn open(columns: &[String], cells: &[String]) -> Self {
        let mut display_lines = Vec::new();
        for (col, cell) in columns.iter().zip(cells.iter()) {
            display_lines.push(col.clone());
            for line in cell.lines() {
                display_lines.push(format!("  {line}"));
            }
            display_lines.push(String::new());
        }
        let display_text = display_lines.join("\n") + "\n";

        let mut obj = serde_json::Map::new();
        for (col, cell) in columns.iter().zip(cells.iter()) {
            obj.insert(col.clone(), infer_json_value(cell));
        }
        let json_text =
            serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_else(|_| "{}".to_string());

        Self {
            display_text,
            json_text,
            scroll_offset: 0,
            horizontal_offset: 0,
            active: true,
        }
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn content(&self) -> &str {
        &self.display_text
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }

    pub fn max_scroll(&self, visible_rows: usize) -> usize {
        self.line_count().saturating_sub(visible_rows.max(1))
    }

    pub fn max_horizontal_scroll(&self, visible_columns: usize) -> usize {
        self.content_width().saturating_sub(visible_columns.max(1))
    }

    pub fn scroll_up_by(&mut self, delta: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(delta);
    }

    pub fn scroll_down_by(&mut self, delta: usize, visible_rows: usize) {
        self.scroll_offset = (self.scroll_offset + delta).min(self.max_scroll(visible_rows));
    }

    pub fn scroll_to_start(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn scroll_to_end(&mut self, visible_rows: usize) {
        self.scroll_offset = self.max_scroll(visible_rows);
    }

    pub fn scroll_left_by(&mut self, delta: usize) {
        self.horizontal_offset = self.horizontal_offset.saturating_sub(delta);
    }

    pub fn scroll_right_by(&mut self, delta: usize, visible_columns: usize) {
        self.horizontal_offset =
            (self.horizontal_offset + delta).min(self.max_horizontal_scroll(visible_columns));
    }

    pub fn clamp_scroll(&mut self, visible_rows: usize, visible_columns: usize) {
        self.scroll_offset = self.scroll_offset.min(self.max_scroll(visible_rows));
        self.horizontal_offset = self
            .horizontal_offset
            .min(self.max_horizontal_scroll(visible_columns));
    }

    pub fn line_count(&self) -> usize {
        self.display_text.lines().count().max(1)
    }

    pub fn content_width(&self) -> usize {
        self.display_text
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(1)
            .max(1)
    }

    pub fn content_for_yank(&self) -> String {
        self.display_text.clone()
    }

    pub fn json_for_yank(&self) -> String {
        self.json_text.clone()
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
    fn empty_cell_displays_column_name_only() {
        let state = RowDetailState::open(&["name".to_string()], &[String::new()]);

        assert!(state.content().contains("name"));
        assert!(state.json_for_yank().contains("\"name\": null"));
        assert!(state.content_for_yank().contains("name"));
    }

    #[test]
    fn number_string_yanks_as_number_json() {
        let state = RowDetailState::open(&["count".to_string()], &["42".to_string()]);

        assert!(state.content().contains("count\n  42"));
        assert!(state.json_for_yank().contains("\"count\": 42"));
    }

    #[test]
    fn number_string_yanks_display_text() {
        let state = RowDetailState::open(&["count".to_string()], &["42".to_string()]);

        let yank = state.content_for_yank();
        assert!(yank.contains("count\n  42"));
    }

    #[test]
    fn boolean_string_yanks_as_boolean() {
        let state = RowDetailState::open(&["active".to_string()], &["true".to_string()]);

        assert!(state.content().contains("active\n  true"));
        assert!(state.json_for_yank().contains("\"active\": true"));
    }

    #[test]
    fn plain_text_displays_indented_and_yanks_as_string() {
        let state = RowDetailState::open(&["title".to_string()], &["hello world".to_string()]);

        assert!(state.content().contains("title\n  hello world"));
        assert!(state.json_for_yank().contains("\"title\": \"hello world\""));
    }

    #[test]
    fn display_text_yank_matches_vertical_render() {
        let state = RowDetailState::open(&["title".to_string()], &["hello world".to_string()]);

        assert_eq!(state.content_for_yank(), state.content());
    }

    #[test]
    fn multiline_cell_value_is_indented() {
        let state = RowDetailState::open(
            &["address".to_string()],
            &["line one\nline two".to_string()],
        );

        let content = state.content();
        assert!(content.contains("address"));
        assert!(content.contains("  line one"));
        assert!(content.contains("  line two"));
    }

    #[test]
    fn content_width_accounts_for_wide_characters_and_clamps_horizontal_scroll() {
        let mut state = RowDetailState::open(&["name".to_string()], &["日本語".to_string()]);

        assert_eq!(state.content_width(), 8);
        assert_eq!(state.max_horizontal_scroll(5), 3);

        state.scroll_right_by(usize::MAX, 5);

        assert_eq!(state.horizontal_offset(), 3);
    }

    #[test]
    fn multiple_columns_build_vertical_display_and_json() {
        let state = RowDetailState::open(
            &["id".to_string(), "name".to_string()],
            &["1".to_string(), "alice".to_string()],
        );

        let content = state.content();
        assert!(content.contains("id\n  1"));
        assert!(content.contains("name\n  alice"));
        let json = state.json_for_yank();
        assert!(json.contains("\"id\": 1"));
        assert!(json.contains("\"name\": \"alice\""));

        assert_eq!(state.content_for_yank(), content);
    }
}
