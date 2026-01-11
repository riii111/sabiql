//! SQL modal sub-reducer: SQL editing and completion.

use std::time::{Duration, Instant};

use crate::app::action::{Action, CursorMove};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::reducers::{char_count, char_to_byte_index};
use crate::app::sql_modal_context::SqlModalStatus;
use crate::app::state::AppState;

/// Handles SQL modal editing and completion actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_sql_modal(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        // Completion navigation
        Action::CompletionNext => {
            if !state.sql_modal.completion.candidates.is_empty() {
                let max = state.sql_modal.completion.candidates.len() - 1;
                state.sql_modal.completion.selected_index =
                    if state.sql_modal.completion.selected_index >= max {
                        0
                    } else {
                        state.sql_modal.completion.selected_index + 1
                    };
            }
            Some(vec![])
        }
        Action::CompletionPrev => {
            if !state.sql_modal.completion.candidates.is_empty() {
                let max = state.sql_modal.completion.candidates.len() - 1;
                state.sql_modal.completion.selected_index =
                    if state.sql_modal.completion.selected_index == 0 {
                        max
                    } else {
                        state.sql_modal.completion.selected_index - 1
                    };
            }
            Some(vec![])
        }
        Action::CompletionDismiss => {
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            Some(vec![])
        }

        // Text editing
        Action::SqlModalInput(c) => {
            state.sql_modal.status = SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, *c);
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalBackspace => {
            state.sql_modal.status = SqlModalStatus::Editing;
            if state.sql_modal.cursor > 0 {
                state.sql_modal.cursor -= 1;
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalDelete => {
            state.sql_modal.status = SqlModalStatus::Editing;
            let total_chars = char_count(&state.sql_modal.content);
            if state.sql_modal.cursor < total_chars {
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalNewLine => {
            state.sql_modal.status = SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, '\n');
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalTab => {
            state.sql_modal.status = SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert_str(byte_idx, "    ");
            state.sql_modal.cursor += 4;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            Some(vec![])
        }
        Action::SqlModalMoveCursor(movement) => {
            let content = &state.sql_modal.content;
            let cursor = state.sql_modal.cursor;
            let total_chars = char_count(content);

            let lines: Vec<(usize, usize)> = {
                let mut result = Vec::new();
                let mut start = 0;
                for line in content.split('\n') {
                    let len = line.chars().count();
                    result.push((start, len));
                    start += len + 1;
                }
                result
            };

            let (current_line, current_col) = {
                let mut line_idx = 0;
                let mut col = cursor;
                for (i, (start, len)) in lines.iter().enumerate() {
                    if cursor >= *start && cursor <= start + len {
                        line_idx = i;
                        col = cursor - start;
                        break;
                    }
                }
                (line_idx, col)
            };

            state.sql_modal.cursor = match movement {
                CursorMove::Left => cursor.saturating_sub(1),
                CursorMove::Right => (cursor + 1).min(total_chars),
                CursorMove::Home => lines.get(current_line).map(|(s, _)| *s).unwrap_or(0),
                CursorMove::End => lines
                    .get(current_line)
                    .map(|(s, l)| s + l)
                    .unwrap_or(total_chars),
                CursorMove::Up => {
                    if current_line == 0 {
                        cursor
                    } else {
                        let (prev_start, prev_len) = lines[current_line - 1];
                        prev_start + current_col.min(prev_len)
                    }
                }
                CursorMove::Down => {
                    if current_line + 1 >= lines.len() {
                        cursor
                    } else {
                        let (next_start, next_len) = lines[current_line + 1];
                        next_start + current_col.min(next_len)
                    }
                }
            };
            Some(vec![])
        }
        Action::SqlModalClear => {
            state.sql_modal.content.clear();
            state.sql_modal.cursor = 0;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            Some(vec![])
        }

        // Modal open/submit
        Action::OpenSqlModal => {
            state.ui.input_mode = InputMode::SqlModal;
            state.sql_modal.status = SqlModalStatus::Editing;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            state.sql_modal.completion.selected_index = 0;
            state.sql_modal.completion_debounce = None;
            if !state.sql_modal.prefetch_started && state.cache.metadata.is_some() {
                Some(vec![Effect::DispatchActions(vec![
                    Action::StartPrefetchAll,
                ])])
            } else {
                Some(vec![])
            }
        }
        Action::SqlModalSubmit => {
            let query = state.sql_modal.content.trim().to_string();
            if !query.is_empty() {
                state.sql_modal.status = SqlModalStatus::Running;
                state.sql_modal.completion.visible = false;
                if let Some(dsn) = &state.runtime.dsn {
                    Some(vec![Effect::ExecuteAdhoc {
                        dsn: dsn.clone(),
                        query,
                    }])
                } else {
                    Some(vec![])
                }
            } else {
                Some(vec![])
            }
        }

        // Completion accept
        Action::CompletionAccept => {
            if state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty()
            {
                let selected_idx = state.sql_modal.completion.selected_index;
                let trigger_pos = state.sql_modal.completion.trigger_position;
                let candidates = std::mem::take(&mut state.sql_modal.completion.candidates);

                if let Some(candidate) = candidates.into_iter().nth(selected_idx) {
                    let start_byte = char_to_byte_index(&state.sql_modal.content, trigger_pos);
                    let end_byte =
                        char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                    state.sql_modal.content.drain(start_byte..end_byte);
                    state
                        .sql_modal
                        .content
                        .insert_str(start_byte, &candidate.text);
                    state.sql_modal.cursor = trigger_pos + candidate.text.chars().count();
                }
                state.sql_modal.completion.visible = false;
                state.sql_modal.completion_debounce = None;
            }
            Some(vec![])
        }

        // Completion trigger/update
        Action::CompletionTrigger => {
            if state.sql_modal.completion_debounce.is_some() {
                state.sql_modal.completion_debounce = None;
                Some(vec![Effect::TriggerCompletion])
            } else {
                Some(vec![])
            }
        }
        Action::CompletionUpdated {
            candidates,
            trigger_position,
            visible,
        } => {
            state.sql_modal.completion.candidates = candidates.clone();
            state.sql_modal.completion.trigger_position = *trigger_position;
            state.sql_modal.completion.visible = *visible;
            state.sql_modal.completion.selected_index = 0;
            Some(vec![])
        }

        _ => None,
    }
}
