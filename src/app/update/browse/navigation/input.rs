use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::text_input::TextEdit;
use crate::update::action::ListMotion;
use crate::update::input::keymap::listed_palette_entries;

pub(in crate::update) fn paste_command_line(state: &mut AppState, text: String) -> Vec<Effect> {
    let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    state.command_line_input.insert_str(&clean);
    state
        .command_line_input
        .update_viewport(state.command_line_visible_width);
    vec![]
}

pub(in crate::update) fn paste_cell_edit(state: &mut AppState, text: String) -> Vec<Effect> {
    let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    state.result_interaction.cell_edit_insert_str(&clean);
    vec![]
}

pub(in crate::update) fn enter_command_line(state: &mut AppState) -> Vec<Effect> {
    state.modal.push_mode(InputMode::CommandLine);
    state.command_line_input.clear();
    vec![]
}

pub(in crate::update) fn exit_command_line(state: &mut AppState) -> Vec<Effect> {
    state.modal.pop_mode();
    vec![]
}

fn select_previous_table_picker_entry(state: &mut AppState) {
    let selected = state.ui.table_picker().selected();
    state
        .ui
        .table_picker_mut()
        .set_selection(selected.saturating_sub(1));
}

pub(in crate::update) fn list_select_table_picker(
    state: &mut AppState,
    motion: ListMotion,
) -> Vec<Effect> {
    match motion {
        ListMotion::Next => {
            let max = state.filtered_tables().len().saturating_sub(1);
            let selected = state.ui.table_picker().selected();
            if selected < max {
                state.ui.table_picker_mut().set_selection(selected + 1);
            }
        }
        ListMotion::Previous => select_previous_table_picker_entry(state),
    }
    vec![]
}

pub(in crate::update) fn list_select_er_table_picker(
    state: &mut AppState,
    motion: ListMotion,
) -> Vec<Effect> {
    match motion {
        ListMotion::Next => {
            let max = state.er_filtered_tables().len().saturating_sub(1);
            let selected = state.ui.er_picker().selected();
            if selected < max {
                state.ui.er_picker_mut().set_selection(selected + 1);
            }
        }
        ListMotion::Previous => {
            let selected = state.ui.er_picker().selected();
            state
                .ui
                .er_picker_mut()
                .set_selection(selected.saturating_sub(1));
        }
    }
    vec![]
}

pub(in crate::update) fn list_select_command_palette(
    state: &mut AppState,
    motion: ListMotion,
) -> Vec<Effect> {
    match motion {
        ListMotion::Next => {
            let max = listed_palette_entries(state).len().saturating_sub(1);
            let selected = state.ui.table_picker().selected();
            if selected < max {
                state.ui.table_picker_mut().set_selection(selected + 1);
            }
        }
        ListMotion::Previous => select_previous_table_picker_entry(state),
    }
    vec![]
}

pub(in crate::update) fn edit_command_line(
    state: &mut AppState,
    edit: TextEdit,
    yank_text: Option<&str>,
) -> String {
    let killed = state.command_line_input.apply_edit(edit, yank_text);
    state
        .command_line_input
        .update_viewport(state.command_line_visible_width);
    killed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::text_input::TextInputState;
    use crate::services::AppServices;
    use crate::update::action::{Action, InputTarget, ListTarget};
    use crate::update::reducer::route;
    use crate::update::test_fixtures::edit_text;
    use std::time::Instant;

    mod paste {
        use super::*;

        #[test]
        fn table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::TablePicker);

            route(
                &mut state,
                Action::Paste {
                    target: InputTarget::Filter,
                    text: "hel\nlo\r\n".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().filter_input().content(), "hello");
        }

        #[test]
        fn table_picker_resets_selection() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker_mut().set_selection(5);

            route(
                &mut state,
                Action::Paste {
                    target: InputTarget::Filter,
                    text: "x".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().selected(), 0);
        }

        #[test]
        fn command_line_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::CommandLine);

            route(
                &mut state,
                Action::Paste {
                    target: InputTarget::CommandLine,
                    text: "qu\nit".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.command_line_input.content(), "quit");
        }

        #[test]
        fn er_table_picker_appends_to_filter() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::ErTablePicker);

            route(
                &mut state,
                Action::Paste {
                    target: InputTarget::ErFilter,
                    text: "public.users".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                state.ui.er_picker().filter_input().content(),
                "public.users"
            );
            assert_eq!(state.ui.er_picker().selected(), 0);
        }

        #[test]
        fn er_table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::ErTablePicker);

            route(
                &mut state,
                Action::Paste {
                    target: InputTarget::ErFilter,
                    text: "public\n.users\r\n".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(
                state.ui.er_picker().filter_input().content(),
                "public.users"
            );
        }

        #[test]
        fn query_history_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::QueryHistoryPicker);
            state.query_history_picker.set_selection_for_test(3);

            route(
                &mut state,
                Action::Paste {
                    target: InputTarget::QueryHistoryFilter,
                    text: "us\ners\r\n".to_string(),
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.query_history_picker.filter_input().content(), "users");
            assert_eq!(state.query_history_picker.selected(), 0);
        }
    }

    mod command_line_return_stack {
        use super::*;

        #[test]
        fn enter_from_normal_and_exit_returns_to_normal() {
            let mut state = AppState::new("test".to_string());

            route(
                &mut state,
                Action::EnterCommandLine,
                Instant::now(),
                &AppServices::stub(),
            );
            assert_eq!(state.input_mode(), InputMode::CommandLine);

            route(
                &mut state,
                Action::ExitCommandLine,
                Instant::now(),
                &AppServices::stub(),
            );
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn enter_from_cell_edit_and_exit_returns_to_cell_edit() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::CellEdit);

            route(
                &mut state,
                Action::EnterCommandLine,
                Instant::now(),
                &AppServices::stub(),
            );
            assert_eq!(state.input_mode(), InputMode::CommandLine);

            route(
                &mut state,
                Action::ExitCommandLine,
                Instant::now(),
                &AppServices::stub(),
            );
            assert_eq!(state.input_mode(), InputMode::CellEdit);
        }
    }

    mod picker_navigation {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};
        use crate::update::modal::base::open_modal_command_palette;
        use std::sync::Arc;

        fn state_with_tables(count: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            let tables: Vec<TableSummary> = (0..count)
                .map(|i| TableSummary::new("public".to_string(), format!("t{i}"), Some(0), false))
                .collect();
            state.session.set_metadata(Some(Arc::new({
                let mut metadata = DatabaseMetadata::new("test".to_string());
                metadata.table_summaries = tables;
                metadata
            })));
            state
        }

        #[test]
        fn table_picker_next_increments() {
            let mut state = state_with_tables(5);
            state.modal.set_mode(InputMode::TablePicker);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Next,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().selected(), 1);
        }

        #[test]
        fn table_picker_next_stops_at_last() {
            let mut state = state_with_tables(3);
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker_mut().set_selection(2);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Next,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().selected(), 2);
        }

        #[test]
        fn table_picker_previous_decrements() {
            let mut state = state_with_tables(5);
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.table_picker_mut().set_selection(3);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Previous,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().selected(), 2);
        }

        #[test]
        fn table_picker_previous_stops_at_zero() {
            let mut state = state_with_tables(5);
            state.modal.set_mode(InputMode::TablePicker);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Previous,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().selected(), 0);
        }

        #[test]
        fn er_picker_next_increments() {
            let mut state = state_with_tables(5);
            state.modal.set_mode(InputMode::ErTablePicker);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Next,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.er_picker().selected(), 1);
        }

        #[test]
        fn er_picker_previous_stops_at_zero() {
            let mut state = state_with_tables(5);
            state.modal.set_mode(InputMode::ErTablePicker);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Previous,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.er_picker().selected(), 0);
        }

        #[test]
        fn command_palette_next_increments() {
            let mut state = AppState::new("test".to_string());
            open_modal_command_palette(&mut state);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Next,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().selected(), 1);
        }

        #[test]
        fn command_palette_previous_stops_at_zero() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::CommandPalette);

            route(
                &mut state,
                Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Previous,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.table_picker().selected(), 0);
        }
    }

    mod readline {
        use super::*;
        use crate::update::action::TextKillDirection;

        #[test]
        fn command_line_yanks_the_latest_killed_text() {
            let mut state = AppState::new("test".to_string());
            state.command_line_input = TextInputState::new("before after", 7);

            edit_text(
                &mut state,
                &Action::TextKill {
                    target: InputTarget::CommandLine,
                    direction: TextKillDirection::ToLineEnd,
                },
                Instant::now(),
            );
            edit_text(
                &mut state,
                &Action::TextYank {
                    target: InputTarget::CommandLine,
                },
                Instant::now(),
            );

            assert_eq!(state.command_line_input.content(), "before after");
        }
    }
}
