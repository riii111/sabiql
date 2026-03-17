mod connections;
mod editors;
mod normal;
mod overlays;
mod pickers;
mod sql_modal;

use crate::app::action::Action;
use crate::app::input_mode::InputMode;
use crate::app::keybindings::KeyCombo;
use crate::app::state::AppState;

use super::Event;

pub fn handle_event(event: Event, state: &AppState) -> Action {
    match event {
        Event::Init => Action::Render,
        Event::Resize(w, h) => Action::Resize(w, h),
        Event::Key(combo) => handle_key_event(combo, state),
        Event::Paste(text) => handle_paste_event(text, state),
    }
}

fn handle_paste_event(text: String, state: &AppState) -> Action {
    match state.input_mode() {
        InputMode::TablePicker
        | InputMode::ErTablePicker
        | InputMode::CommandLine
        | InputMode::CellEdit
        | InputMode::ConnectionSetup
        | InputMode::SqlModal
        | InputMode::QueryHistoryPicker => Action::Paste(text),
        _ => Action::None,
    }
}

fn handle_key_event(combo: KeyCombo, state: &AppState) -> Action {
    match state.input_mode() {
        InputMode::Normal => normal::handle_normal_mode(combo, state),
        InputMode::CommandLine => editors::handle_command_line_mode(combo),
        InputMode::CellEdit => editors::handle_cell_edit_keys(combo),
        InputMode::TablePicker => pickers::handle_table_picker_keys(combo),
        InputMode::CommandPalette => pickers::handle_command_palette_keys(combo),
        InputMode::Help => overlays::handle_help_keys(combo),
        InputMode::SqlModal => {
            let completion_visible = state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty();
            sql_modal::handle_sql_modal_keys(combo, completion_visible, state.sql_modal.status())
        }
        InputMode::ConnectionSetup => connections::handle_connection_setup_keys(combo, state),
        InputMode::ConnectionError => connections::handle_connection_error_keys(combo),
        InputMode::ConfirmDialog => overlays::handle_confirm_dialog_keys(combo),
        InputMode::ConnectionSelector => connections::handle_connection_selector_keys(combo),
        InputMode::ErTablePicker => pickers::handle_er_table_picker_keys(combo),
        InputMode::QueryHistoryPicker => pickers::handle_query_history_picker_keys(combo),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keybindings::{Key, KeyCombo};

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    mod mode_dispatch {
        use super::*;

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(mode);
            state
        }

        #[test]
        fn normal_mode_routes_to_normal_handler() {
            let state = make_state(InputMode::Normal);

            // 'q' in Normal mode should quit
            let result = handle_key_event(combo(Key::Char('q')), &state);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn sql_modal_mode_routes_to_sql_modal_handler() {
            let state = make_state(InputMode::SqlModal);

            // Esc in SqlModal (Normal mode, the default) should close modal
            let result = handle_key_event(combo(Key::Esc), &state);

            assert!(matches!(result, Action::CloseSqlModal));
        }
    }

    mod paste_event {
        use super::*;

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(mode);
            state
        }

        #[test]
        fn paste_event_in_sql_modal_returns_paste_action() {
            let state = make_state(InputMode::SqlModal);

            let result = handle_paste_event("hello".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "hello"));
        }

        #[test]
        fn paste_event_in_table_picker_returns_paste_action() {
            let state = make_state(InputMode::TablePicker);

            let result = handle_paste_event("world".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "world"));
        }

        #[test]
        fn paste_event_in_er_table_picker_returns_paste_action() {
            let state = make_state(InputMode::ErTablePicker);

            let result = handle_paste_event("public.users".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "public.users"));
        }

        #[test]
        fn paste_event_in_query_history_picker_returns_paste_action() {
            let state = make_state(InputMode::QueryHistoryPicker);

            let result = handle_paste_event("users".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "users"));
        }

        #[test]
        fn paste_event_in_normal_mode_returns_none() {
            let state = make_state(InputMode::Normal);

            let result = handle_paste_event("text".to_string(), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn paste_event_in_help_mode_returns_none() {
            let state = make_state(InputMode::Help);

            let result = handle_paste_event("text".to_string(), &state);

            assert!(matches!(result, Action::None));
        }
    }
}
