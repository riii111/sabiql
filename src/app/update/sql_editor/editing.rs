use std::time::{Duration, Instant};

use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::shared::text_input::TextEdit;
use crate::model::sql_editor::modal::{SqlModalStatus, sql_modal_visible_rows};
use crate::update::action::{Action, CursorMove};
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_editing(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        // Clipboard paste
        Action::Paste(text) if state.modal.active_mode() == InputMode::SqlModal => {
            if !matches!(state.sql_modal.status(), SqlModalStatus::Editing) {
                return DispatchResult::handled();
            }
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            state.sql_modal.editor.insert_str(&normalized);
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height()));
            state
                .sql_modal
                .schedule_completion_after_dismiss(now + Duration::from_millis(100));
            state.sql_modal.enter_editing();
            DispatchResult::handled()
        }

        // Text editing
        Action::SqlModalNewLine => {
            state.sql_modal.enter_editing();
            state.sql_modal.editor.insert_newline();
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height()));
            state
                .sql_modal
                .schedule_completion(now + Duration::from_millis(100));
            DispatchResult::handled()
        }
        Action::SqlModalInsertTab => {
            state.sql_modal.enter_editing();
            state.sql_modal.editor.insert_tab();
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height()));
            state
                .sql_modal
                .schedule_completion(now + Duration::from_millis(100));
            DispatchResult::handled()
        }
        Action::SqlModalClear => {
            state.sql_modal.editor.clear();
            state.sql_modal.reset_completion();
            state.ui.set_key_sequence(KeySequenceState::Idle);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

pub(in crate::update) fn edit_text(
    state: &mut AppState,
    edit: TextEdit,
    yank_text: Option<&str>,
    now: Instant,
) -> String {
    let visible_rows = sql_modal_visible_rows(state.ui.terminal_height());
    if let TextEdit::MoveCursor(movement) = edit {
        match movement {
            CursorMove::ViewportTop | CursorMove::ViewportMiddle | CursorMove::ViewportBottom => {
                state
                    .sql_modal
                    .editor
                    .move_cursor_to_viewport_position(movement, visible_rows);
            }
            _ => state.sql_modal.editor.move_cursor(movement),
        }
        state.sql_modal.editor.update_scroll(visible_rows);
        state.ui.set_key_sequence(KeySequenceState::Idle);
        return String::new();
    }

    state.sql_modal.enter_editing();
    let killed = state.sql_modal.editor.apply_edit(edit, yank_text);
    state.sql_modal.editor.update_scroll(visible_rows);
    state
        .sql_modal
        .schedule_completion(now + Duration::from_millis(100));
    killed
}
