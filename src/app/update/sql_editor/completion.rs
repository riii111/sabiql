use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextInputLike;
use crate::model::sql_editor::modal::sql_modal_visible_rows;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_completion(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        // Completion navigation
        Action::CompletionNext => {
            state.sql_modal.completion_next();
            DispatchResult::handled()
        }
        Action::CompletionPrev => {
            state.sql_modal.completion_prev();
            DispatchResult::handled()
        }
        Action::CompletionDismiss => {
            state.sql_modal.dismiss_completion();
            DispatchResult::handled()
        }
        // Completion accept
        Action::CompletionAccept => {
            if let Some((trigger_pos, replacement)) =
                state.sql_modal.selected_completion_replacement()
            {
                if state.sql_modal.editor.cursor() < trigger_pos {
                    state.sql_modal.dismiss_completion();
                    return DispatchResult::handled();
                }

                let start_byte = state.sql_modal.editor.char_to_byte_index(trigger_pos);
                let end_byte = state
                    .sql_modal
                    .editor
                    .char_to_byte_index(state.sql_modal.editor.cursor());
                // Manually manipulate the underlying content for drain + insert_str at byte level.
                // This is the one place where we need byte-level access that MultiLineInputState
                // doesn't directly support, so we rebuild via set_content.
                let mut content = state.sql_modal.editor.content().to_string();
                content.drain(start_byte..end_byte);
                content.insert_str(start_byte, &replacement);
                let new_cursor = trigger_pos + replacement.chars().count();
                state
                    .sql_modal
                    .editor
                    .set_content_with_cursor(content, new_cursor);
                state
                    .sql_modal
                    .editor
                    .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
                state.sql_modal.dismiss_completion();
            }
            DispatchResult::handled()
        }

        // Completion trigger/update
        Action::CompletionTrigger => DispatchResult::handled_with(vec![Effect::TriggerCompletion]),
        Action::CompletionUpdated {
            candidates,
            trigger_position,
            visible,
        } => {
            state
                .sql_modal
                .apply_completion_update(candidates, *trigger_position, *visible);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
