use crate::app::action::{Action, InputTarget};
use crate::app::effect::Effect;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

pub fn reduce(state: &mut AppState, action: &Action) -> Option<Vec<Effect>> {
    match action {
        Action::Paste(text) => match state.modal.active_mode() {
            InputMode::TablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.table_picker.filter_input.push_str(&clean);
                state.ui.table_picker.reset();
                Some(vec![])
            }
            InputMode::ErTablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.er_picker.filter_input.push_str(&clean);
                state.ui.er_picker.reset();
                Some(vec![])
            }
            InputMode::CommandLine => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.command_line_input.push_str(&clean);
                Some(vec![])
            }
            InputMode::CellEdit => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state
                    .result_interaction
                    .cell_edit_input_mut()
                    .insert_str(&clean);
                Some(vec![])
            }
            InputMode::QueryHistoryPicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.query_history_picker.filter_input.insert_str(&clean);
                state.query_history_picker.selected = 0;
                state.query_history_picker.scroll_offset = 0;
                Some(vec![])
            }
            _ => None,
        },

        Action::TextInput {
            target: InputTarget::Filter,
            ch: c,
        } => {
            state.ui.table_picker.filter_input.push(*c);
            state.ui.table_picker.reset();
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::Filter,
        } => {
            state.ui.table_picker.filter_input.pop();
            state.ui.table_picker.reset();
            Some(vec![])
        }

        Action::EnterCommandLine => {
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.clear();
            Some(vec![])
        }
        Action::ExitCommandLine => {
            state.modal.pop_mode();
            Some(vec![])
        }
        Action::TextInput {
            target: InputTarget::CommandLine,
            ch: c,
        } => {
            state.command_line_input.push(*c);
            Some(vec![])
        }
        Action::TextBackspace {
            target: InputTarget::CommandLine,
        } => {
            state.command_line_input.pop();
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::reducers::navigation::reduce_navigation;
    use crate::app::services::AppServices;
    use std::time::Instant;

    mod paste {
        use super::*;

        #[test]
        fn paste_in_table_picker_appends_text() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::TablePicker);

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("hello".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.table_picker.filter_input, "hello");
        }

        #[test]
        fn paste_in_table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::TablePicker);

            reduce_navigation(
                &mut state,
                &Action::Paste("hel\nlo\r\n".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.table_picker.filter_input, "hello");
        }

        #[test]
        fn paste_in_table_picker_resets_selection() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker.set_selection(5);

            reduce_navigation(
                &mut state,
                &Action::Paste("x".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.table_picker.selected(), 0);
        }

        #[test]
        fn paste_in_command_line_appends_text() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::CommandLine);

            reduce_navigation(
                &mut state,
                &Action::Paste("quit".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.command_line_input, "quit");
        }

        #[test]
        fn paste_in_command_line_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::CommandLine);

            reduce_navigation(
                &mut state,
                &Action::Paste("qu\nit".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.command_line_input, "quit");
        }

        #[test]
        fn paste_in_normal_mode_returns_none() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::Normal);

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("text".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_none());
        }

        #[test]
        fn paste_in_er_table_picker_appends_to_er_filter() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::ErTablePicker);

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("public.users".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.er_picker.filter_input, "public.users");
            assert_eq!(state.ui.er_picker.selected(), 0);
        }

        #[test]
        fn paste_in_er_table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::ErTablePicker);

            reduce_navigation(
                &mut state,
                &Action::Paste("public\n.users\r\n".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.er_picker.filter_input, "public.users");
        }

        #[test]
        fn paste_in_query_history_picker_appends_to_filter() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::QueryHistoryPicker);
            state.query_history_picker.selected = 3;

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("users".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.query_history_picker.filter_input.content(), "users");
            assert_eq!(state.query_history_picker.selected, 0);
        }

        #[test]
        fn paste_in_query_history_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::QueryHistoryPicker);

            reduce_navigation(
                &mut state,
                &Action::Paste("us\ners\r\n".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.query_history_picker.filter_input.content(), "users");
        }
    }

    mod command_line_return_stack {
        use super::*;

        #[test]
        fn enter_from_normal_and_exit_returns_to_normal() {
            let mut state = AppState::new("test".to_string());

            reduce_navigation(
                &mut state,
                &Action::EnterCommandLine,
                &AppServices::stub(),
                Instant::now(),
            );
            assert_eq!(state.input_mode(), InputMode::CommandLine);

            reduce_navigation(
                &mut state,
                &Action::ExitCommandLine,
                &AppServices::stub(),
                Instant::now(),
            );
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn enter_from_cell_edit_and_exit_returns_to_cell_edit() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::CellEdit);

            reduce_navigation(
                &mut state,
                &Action::EnterCommandLine,
                &AppServices::stub(),
                Instant::now(),
            );
            assert_eq!(state.input_mode(), InputMode::CommandLine);

            reduce_navigation(
                &mut state,
                &Action::ExitCommandLine,
                &AppServices::stub(),
                Instant::now(),
            );
            assert_eq!(state.input_mode(), InputMode::CellEdit);
        }
    }
}
