use crate::model::shared::cursor::CursorMove;
use crate::model::shared::text_input::{TextEdit, TextInputState};
use crate::update::input::keymap::KeyContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpState {
    origin: Option<KeyContext>,
    filter: TextInputState,
    mode: HelpMode,
    scroll_offset: usize,
    horizontal_offset: usize,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            origin: None,
            filter: TextInputState::default(),
            mode: HelpMode::Viewing,
            scroll_offset: 0,
            horizontal_offset: 0,
        }
    }
}

impl HelpState {
    pub fn open(&mut self, origin: KeyContext) {
        self.origin = Some(origin);
        self.filter.clear();
        self.mode = HelpMode::Viewing;
        self.reset_offsets();
    }

    pub fn close(&mut self) {
        self.filter.clear();
        self.mode = HelpMode::Viewing;
        self.reset_offsets();
    }

    pub fn origin(&self) -> Option<&KeyContext> {
        self.origin.as_ref()
    }

    pub fn filter(&self) -> &TextInputState {
        &self.filter
    }

    pub fn mode(&self) -> HelpMode {
        self.mode
    }

    pub fn enter_filter_editing(&mut self) {
        self.mode = HelpMode::EditingFilter;
        self.reset_offsets();
    }

    pub fn exit_filter_editing(&mut self) {
        self.mode = HelpMode::Viewing;
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn set_horizontal_offset(&mut self, offset: usize) {
        self.horizontal_offset = offset;
    }

    pub fn reset_offsets(&mut self) {
        self.scroll_offset = 0;
        self.horizontal_offset = 0;
    }

    pub fn insert_filter_char(&mut self, ch: char) {
        self.filter.insert_char(ch);
        self.reset_offsets();
    }

    pub fn backspace_filter(&mut self) {
        self.filter.backspace();
        self.reset_offsets();
    }

    pub fn edit_filter<R>(&mut self, edit: impl FnOnce(&mut TextInputState) -> R) -> R {
        let result = edit(&mut self.filter);
        self.reset_offsets();
        result
    }

    pub fn apply_filter_edit(&mut self, edit: TextEdit, yank_text: Option<&str>) -> String {
        if let TextEdit::MoveCursor(direction) = edit {
            self.move_filter_cursor(direction);
            return String::new();
        }
        self.edit_filter(|input| input.apply_edit(edit, yank_text))
    }

    pub fn move_filter_cursor(&mut self, direction: CursorMove) {
        self.filter.move_cursor(direction);
    }

    pub fn clamp_offsets(&mut self, max_scroll: usize, max_horizontal_scroll: usize) {
        self.scroll_offset = self.scroll_offset.min(max_scroll);
        self.horizontal_offset = self.horizontal_offset.min(max_horizontal_scroll);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpMode {
    #[default]
    Viewing,
    EditingFilter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_input_resets_offsets() {
        let mut state = HelpState::default();
        state.set_scroll_offset(10);
        state.set_horizontal_offset(4);

        state.insert_filter_char('c');

        assert_eq!(state.filter().content(), "c");
        assert_eq!(state.scroll_offset(), 0);
        assert_eq!(state.horizontal_offset(), 0);
    }

    #[test]
    fn filter_mode_resets_when_help_is_closed() {
        let mut state = HelpState::default();
        state.enter_filter_editing();

        state.close();

        assert_eq!(state.mode(), HelpMode::Viewing);
    }

    #[test]
    fn filter_mode_returns_to_viewing_without_clearing_the_filter() {
        let mut state = HelpState::default();
        state.insert_filter_char('c');
        state.enter_filter_editing();

        state.exit_filter_editing();

        assert_eq!(state.mode(), HelpMode::Viewing);
        assert_eq!(state.filter().content(), "c");
    }

    #[test]
    fn entering_filter_mode_resets_scroll_offsets() {
        let mut state = HelpState::default();
        state.set_scroll_offset(10);
        state.set_horizontal_offset(4);

        state.enter_filter_editing();

        assert_eq!(state.mode(), HelpMode::EditingFilter);
        assert_eq!(state.scroll_offset(), 0);
        assert_eq!(state.horizontal_offset(), 0);
    }
}
