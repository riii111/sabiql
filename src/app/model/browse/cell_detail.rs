use crate::model::shared::detail_view::{DetailContentState, DetailSearchState};
use crate::model::shared::multi_line_input::MultiLineInputState;
use crate::model::shared::text_input::TextInputLike;
use crate::update::action::CursorMove;

const DEFAULT_VISIBLE_ROWS: usize = 8;
const DEFAULT_VIEWPORT_WIDTH: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellDetailMode {
    #[default]
    Viewing,
    Editing,
    Searching,
}

#[derive(Debug, Clone)]
pub struct CellDetailState {
    detail: DetailContentState,
    mode: CellDetailMode,
    editor: MultiLineInputState,
    search: DetailSearchState,
    visible_rows: usize,
    viewport_width: usize,
    active: bool,
}

impl Default for CellDetailState {
    fn default() -> Self {
        Self {
            detail: DetailContentState::default(),
            mode: CellDetailMode::Viewing,
            editor: MultiLineInputState::default(),
            search: DetailSearchState::default(),
            visible_rows: DEFAULT_VISIBLE_ROWS,
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
            active: false,
        }
    }
}

impl CellDetailState {
    pub fn open(
        row: usize,
        col: usize,
        column_name: String,
        original_content: String,
        display_content: String,
    ) -> Self {
        Self {
            detail: DetailContentState::new(
                row,
                col,
                column_name,
                original_content,
                display_content.clone(),
            ),
            editor: MultiLineInputState::new(display_content, 0),
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

    pub fn mode(&self) -> CellDetailMode {
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

    pub fn content(&self) -> &str {
        self.editor.content()
    }

    pub fn original_content(&self) -> &str {
        self.detail.original_content()
    }

    pub fn display_content(&self) -> &str {
        self.detail.content()
    }

    pub fn editor(&self) -> &MultiLineInputState {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut MultiLineInputState {
        &mut self.editor
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
        self.editor.update_scroll(self.visible_rows);
    }

    pub fn enter_search(&mut self) {
        self.mode = CellDetailMode::Searching;
        self.search.reset();
        self.search.activate();
    }

    pub fn exit_search(&mut self) {
        self.search.deactivate();
        self.mode = CellDetailMode::Viewing;
    }

    pub fn enter_edit(&mut self) {
        self.search.deactivate();
        self.mode = CellDetailMode::Editing;
    }

    pub fn exit_edit(&mut self) {
        self.mode = CellDetailMode::Viewing;
    }

    pub fn move_editor_cursor(&mut self, direction: CursorMove) {
        match direction {
            CursorMove::ViewportTop | CursorMove::ViewportMiddle | CursorMove::ViewportBottom => {
                self.editor
                    .move_cursor_to_viewport_position(direction, self.visible_rows);
            }
            _ => self.editor.move_cursor(direction),
        }
        self.editor.update_scroll(self.visible_rows);
    }

    pub fn update_editor_scroll(&mut self) {
        self.editor.update_scroll(self.visible_rows);
    }

    pub fn scroll_to_match(&mut self) {
        let Some(&match_pos) = self.search.matches().get(self.search.current_match()) else {
            return;
        };
        self.editor.set_cursor(match_pos);
        self.editor.update_scroll(self.visible_rows);
    }

    pub fn has_pending_changes(&self) -> bool {
        let content = self.editor.content();
        content != self.original_content() && content != self.display_content()
    }
}
