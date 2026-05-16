use std::time::{Duration, Instant};

use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::sql_editor::modal::{SqlModalStatus, sql_modal_visible_rows};
use crate::update::action::{Action, CursorMove, InputTarget};
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
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state
                .sql_modal
                .schedule_completion_after_dismiss(now + Duration::from_millis(100));
            state.sql_modal.enter_editing();
            DispatchResult::handled()
        }

        // Text editing
        Action::TextInput {
            target: InputTarget::SqlModal,
            ch: c,
        } => {
            state.sql_modal.enter_editing();
            state.sql_modal.editor.insert_char(*c);
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state
                .sql_modal
                .schedule_completion(now + Duration::from_millis(100));
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::SqlModal,
        } => {
            state.sql_modal.enter_editing();
            state.sql_modal.editor.backspace();
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state
                .sql_modal
                .schedule_completion(now + Duration::from_millis(100));
            DispatchResult::handled()
        }
        Action::TextDelete {
            target: InputTarget::SqlModal,
        } => {
            state.sql_modal.enter_editing();
            state.sql_modal.editor.delete();
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state
                .sql_modal
                .schedule_completion(now + Duration::from_millis(100));
            DispatchResult::handled()
        }
        Action::SqlModalNewLine => {
            state.sql_modal.enter_editing();
            state.sql_modal.editor.insert_newline();
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state
                .sql_modal
                .schedule_completion(now + Duration::from_millis(100));
            DispatchResult::handled()
        }
        Action::SqlModalTab => {
            state.sql_modal.enter_editing();
            state.sql_modal.editor.insert_tab();
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state
                .sql_modal
                .schedule_completion(now + Duration::from_millis(100));
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: movement,
        } => {
            match movement {
                CursorMove::ViewportTop
                | CursorMove::ViewportMiddle
                | CursorMove::ViewportBottom => {
                    state.sql_modal.editor.move_cursor_to_viewport_position(
                        *movement,
                        sql_modal_visible_rows(state.ui.terminal_height),
                    );
                }
                _ => state.sql_modal.editor.move_cursor(*movement),
            }
            state
                .sql_modal
                .editor
                .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
            state.ui.key_sequence = KeySequenceState::Idle;
            DispatchResult::handled()
        }
        Action::SqlModalClear => {
            state.sql_modal.editor.clear();
            state.sql_modal.reset_completion();
            state.ui.key_sequence = KeySequenceState::Idle;
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
