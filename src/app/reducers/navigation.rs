use std::time::Instant;

use crate::app::action::{Action, ConnectionsLoadedPayload};
use crate::app::confirm_dialog_state::ConfirmIntent;
use crate::app::effect::Effect;
use crate::app::focused_pane::FocusedPane;
use crate::app::input_mode::InputMode;
use crate::app::inspector_tab::InspectorTab;
use crate::app::palette::palette_command_count;
use crate::app::services::AppServices;
use crate::app::state::AppState;
use crate::app::viewport::{calculate_next_column_offset, calculate_prev_column_offset};

fn inspector_total_items(state: &AppState, services: &AppServices) -> usize {
    state
        .session
        .table_detail()
        .map(|t| match state.ui.inspector_tab {
            InspectorTab::Info => 5,
            InspectorTab::Columns => t.columns.len(),
            InspectorTab::Indexes => t.indexes.len(),
            InspectorTab::ForeignKeys => t.foreign_keys.len(),
            InspectorTab::Rls => t.rls.as_ref().map_or(1, |rls| {
                let mut lines = 1;
                if !rls.policies.is_empty() {
                    lines += 2;
                    for policy in &rls.policies {
                        lines += 1;
                        if policy.qual.is_some() {
                            lines += 1;
                        }
                    }
                }
                lines
            }),
            InspectorTab::Triggers => t.triggers.len(),
            InspectorTab::Ddl => services.ddl_generator.ddl_line_count(t),
        })
        .unwrap_or(0)
}

fn inspector_max_scroll(state: &AppState, services: &AppServices) -> usize {
    let visible = match state.ui.inspector_tab {
        InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
        _ => state.inspector_visible_rows(),
    };
    inspector_total_items(state, services).saturating_sub(visible)
}

fn explorer_item_count(state: &AppState) -> usize {
    state.tables().len()
}

pub fn reduce_navigation(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        Action::SetFocusedPane(pane) => {
            if *pane != FocusedPane::Result {
                state.result_interaction.reset_interaction();
                if state.modal.active_mode() == InputMode::CellEdit {
                    state.modal.set_mode(InputMode::Normal);
                }
            }
            state.ui.focused_pane = *pane;
            Some(vec![])
        }
        Action::ToggleFocus => {
            let was_focus = state.ui.focus_mode;
            state.toggle_focus();
            if was_focus {
                state.result_interaction.reset_interaction();
            }
            Some(vec![])
        }
        Action::ToggleReadOnly => {
            if state.session.read_only {
                // RO→RW: confirm dialog (dangerous direction)
                state.confirm_dialog.open(
                    "Disable Read-Only",
                    "Switch to read-write mode? Write operations will be allowed.",
                    ConfirmIntent::DisableReadOnly,
                );
                state.modal.push_mode(InputMode::ConfirmDialog);
            } else {
                // RW→RO: immediate (safe direction)
                state.session.read_only = true;
            }
            Some(vec![])
        }
        Action::InspectorNextTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.next();
            Some(vec![])
        }
        Action::InspectorPrevTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.prev();
            Some(vec![])
        }

        // Clipboard paste
        Action::Paste(text) => match state.modal.active_mode() {
            InputMode::TablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.filter_input.push_str(&clean);
                state.ui.reset_picker_selection();
                Some(vec![])
            }
            InputMode::ErTablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.er_filter_input.push_str(&clean);
                state.ui.reset_er_picker_selection();
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
            _ => None,
        },

        // Filter
        Action::FilterInput(c) => {
            state.ui.filter_input.push(*c);
            state.ui.reset_picker_selection();
            Some(vec![])
        }
        Action::FilterBackspace => {
            state.ui.filter_input.pop();
            state.ui.reset_picker_selection();
            Some(vec![])
        }

        // Command Line
        Action::EnterCommandLine => {
            state.modal.push_mode(InputMode::CommandLine);
            state.command_line_input.clear();
            Some(vec![])
        }
        Action::ExitCommandLine => {
            state.modal.pop_mode();
            Some(vec![])
        }
        Action::CommandLineInput(c) => {
            state.command_line_input.push(*c);
            Some(vec![])
        }
        Action::CommandLineBackspace => {
            state.command_line_input.pop();
            Some(vec![])
        }

        // Selection
        Action::SelectNext => {
            match state.modal.active_mode() {
                InputMode::TablePicker => {
                    let max = state.filtered_tables().len().saturating_sub(1);
                    if state.ui.picker_selected < max {
                        state.ui.set_picker_selection(state.ui.picker_selected + 1);
                    }
                }
                InputMode::ErTablePicker => {
                    let max = state.er_filtered_tables().len().saturating_sub(1);
                    if state.ui.er_picker_selected < max {
                        state
                            .ui
                            .set_er_picker_selection(state.ui.er_picker_selected + 1);
                    }
                }
                InputMode::CommandPalette => {
                    let max = palette_command_count() - 1;
                    if state.ui.picker_selected < max {
                        state.ui.set_picker_selection(state.ui.picker_selected + 1);
                    }
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer {
                        let len = state.tables().len();
                        if len > 0 && state.ui.explorer_selected < len - 1 {
                            state
                                .ui
                                .set_explorer_selection(Some(state.ui.explorer_selected + 1));
                        }
                    }
                }
                _ => {}
            }
            Some(vec![])
        }
        Action::SelectPrevious => {
            match state.modal.active_mode() {
                InputMode::TablePicker | InputMode::CommandPalette => {
                    state
                        .ui
                        .set_picker_selection(state.ui.picker_selected.saturating_sub(1));
                }
                InputMode::ErTablePicker => {
                    state
                        .ui
                        .set_er_picker_selection(state.ui.er_picker_selected.saturating_sub(1));
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer && !state.tables().is_empty()
                    {
                        let new_idx = state.ui.explorer_selected.saturating_sub(1);
                        state.ui.set_explorer_selection(Some(new_idx));
                    }
                }
                _ => {}
            }
            Some(vec![])
        }
        Action::SelectFirst => {
            if state.ui.focused_pane == FocusedPane::Explorer && !state.tables().is_empty() {
                state.ui.set_explorer_selection(Some(0));
            }
            Some(vec![])
        }
        Action::SelectLast => {
            if state.ui.focused_pane == FocusedPane::Explorer {
                let len = state.tables().len();
                if len > 0 {
                    state.ui.set_explorer_selection(Some(len - 1));
                }
            }
            Some(vec![])
        }
        Action::SelectMiddle => {
            if state.ui.focused_pane == FocusedPane::Explorer {
                let len = explorer_item_count(state);
                if len > 0 {
                    let target = len / 2;
                    state.ui.set_explorer_selection(Some(target));
                    let visible = state.ui.explorer_visible_items();
                    if visible > 0 {
                        let max_offset = len.saturating_sub(visible);
                        state.ui.explorer_scroll_offset =
                            target.saturating_sub(visible / 2).min(max_offset);
                    }
                }
            }
            Some(vec![])
        }

        // Explorer page scroll (selection-based)
        Action::SelectHalfPageDown => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = (visible / 2).max(1);
            let max_idx = len.saturating_sub(1);
            let new_idx = (state.ui.explorer_selected + delta).min(max_idx);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }
        Action::SelectHalfPageUp => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = (visible / 2).max(1);
            let new_idx = state.ui.explorer_selected.saturating_sub(delta);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }
        Action::SelectFullPageDown => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = visible.max(1);
            let max_idx = len.saturating_sub(1);
            let new_idx = (state.ui.explorer_selected + delta).min(max_idx);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }
        Action::SelectFullPageUp => {
            let len = explorer_item_count(state);
            if len == 0 {
                return Some(vec![]);
            }
            let visible = state.ui.explorer_visible_items();
            let delta = visible.max(1);
            let new_idx = state.ui.explorer_selected.saturating_sub(delta);
            state.ui.set_explorer_selection(Some(new_idx));
            Some(vec![])
        }

        // Inspector Scroll
        Action::InspectorScrollUp => {
            state.ui.inspector_scroll_offset = state.ui.inspector_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::InspectorScrollDown => {
            let max_offset = inspector_max_scroll(state, services);
            if state.ui.inspector_scroll_offset < max_offset {
                state.ui.inspector_scroll_offset += 1;
            }
            Some(vec![])
        }
        Action::InspectorScrollTop => {
            state.ui.inspector_scroll_offset = 0;
            Some(vec![])
        }
        Action::InspectorScrollBottom => {
            state.ui.inspector_scroll_offset = inspector_max_scroll(state, services);
            Some(vec![])
        }
        Action::InspectorScrollMiddle => {
            let max = inspector_max_scroll(state, services);
            state.ui.inspector_scroll_offset = max / 2;
            Some(vec![])
        }
        Action::InspectorScrollHalfPageDown => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = (visible / 2).max(1);
            let max = inspector_max_scroll(state, services);
            state.ui.inspector_scroll_offset = (state.ui.inspector_scroll_offset + delta).min(max);
            Some(vec![])
        }
        Action::InspectorScrollHalfPageUp => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = (visible / 2).max(1);
            state.ui.inspector_scroll_offset =
                state.ui.inspector_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }
        Action::InspectorScrollFullPageDown => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = visible.max(1);
            let max = inspector_max_scroll(state, services);
            state.ui.inspector_scroll_offset = (state.ui.inspector_scroll_offset + delta).min(max);
            Some(vec![])
        }
        Action::InspectorScrollFullPageUp => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = visible.max(1);
            state.ui.inspector_scroll_offset =
                state.ui.inspector_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }
        Action::InspectorScrollLeft => {
            state.ui.inspector_horizontal_offset =
                calculate_prev_column_offset(state.ui.inspector_horizontal_offset);
            Some(vec![])
        }
        Action::InspectorScrollRight => {
            let plan = &state.ui.inspector_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.inspector_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.inspector_horizontal_offset,
                plan.column_count,
            );
            Some(vec![])
        }

        // Explorer Scroll
        Action::ExplorerScrollLeft => {
            state.ui.explorer_horizontal_offset =
                state.ui.explorer_horizontal_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::ExplorerScrollRight => {
            let max_name_width = state
                .tables()
                .iter()
                .map(|t| t.qualified_name().len())
                .max()
                .unwrap_or(0);
            if state.ui.explorer_horizontal_offset < max_name_width {
                state.ui.explorer_horizontal_offset += 1;
            }
            Some(vec![])
        }

        Action::ConnectionListSelectNext => {
            let len = state.connection_list_items().len();
            let next = state.ui.connection_list_selected + 1;
            if next < len {
                state.ui.set_connection_list_selection(Some(next));
            }
            Some(vec![])
        }
        Action::ConnectionListSelectPrevious => {
            if state.ui.connection_list_selected > 0 {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected - 1));
            }
            Some(vec![])
        }
        Action::ConnectionsLoaded(ConnectionsLoadedPayload {
            profiles,
            services,
            service_file_path,
            profile_load_warning,
            service_load_warning,
        }) => {
            let mut sorted = profiles.clone();
            sorted.sort_by(|a, b| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            });
            state.set_connections_and_services(sorted, services.clone());
            state.runtime.service_file_path = service_file_path.clone();

            if let Some(warning) = profile_load_warning {
                state.messages.set_error_at(warning.clone(), now);
            }
            if let Some(warning) = service_load_warning {
                state.messages.set_error_at(warning.clone(), now);
            }

            let list_len = state.connection_list_items().len();
            if list_len == 0 {
                state.ui.set_connection_list_selection(Some(0));
            } else if state.ui.connection_list_selected >= list_len {
                state
                    .ui
                    .set_connection_list_selection(Some(list_len.saturating_sub(1)));
            } else {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected));
            }
            Some(vec![])
        }
        Action::ConfirmConnectionSelection => {
            use crate::app::connection_list::ConnectionListItem;
            let selected_idx = state.ui.connection_list_selected;

            let effect = match state.connection_list_items().get(selected_idx) {
                Some(ConnectionListItem::Profile(i)) => state
                    .connections()
                    .get(*i)
                    .filter(|c| state.session.active_connection_id.as_ref() != Some(&c.id))
                    .map(|_| Effect::SwitchConnection {
                        connection_index: *i,
                    }),
                Some(ConnectionListItem::Service(i)) => {
                    Some(Effect::SwitchToService { service_index: *i })
                }
                _ => None,
            };

            state.modal.set_mode(InputMode::Normal);

            match effect {
                Some(e) => Some(vec![e]),
                None => Some(vec![]),
            }
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::Effect;
    use crate::app::services::AppServices;
    use crate::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};
    use std::time::Instant;

    fn create_test_profile(name: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: ConnectionId::new(),
            name: ConnectionName::new(name).unwrap(),
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            ssl_mode: SslMode::Prefer,
        }
    }

    mod toggle_read_only {
        use super::*;

        #[test]
        fn rw_to_ro_switches_immediately() {
            let mut state = AppState::new("test".to_string());
            assert!(!state.session.read_only);

            reduce_navigation(
                &mut state,
                &Action::ToggleReadOnly,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.session.read_only);
            assert_eq!(state.input_mode(), InputMode::Normal);
        }

        #[test]
        fn ro_to_rw_opens_confirm_dialog() {
            let mut state = AppState::new("test".to_string());
            state.session.read_only = true;

            reduce_navigation(
                &mut state,
                &Action::ToggleReadOnly,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.session.read_only);
            assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
            assert!(matches!(
                state.confirm_dialog.intent(),
                Some(crate::app::confirm_dialog_state::ConfirmIntent::DisableReadOnly)
            ));
        }
    }

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
            assert_eq!(state.ui.filter_input, "hello");
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

            assert_eq!(state.ui.filter_input, "hello");
        }

        #[test]
        fn paste_in_table_picker_resets_selection() {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(InputMode::TablePicker);
            state.ui.picker_selected = 5;

            reduce_navigation(
                &mut state,
                &Action::Paste("x".to_string()),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.picker_selected, 0);
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
            assert_eq!(state.ui.er_filter_input, "public.users");
            assert_eq!(state.ui.er_picker_selected, 0);
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

            assert_eq!(state.ui.er_filter_input, "public.users");
        }
    }

    mod connection_list_navigation {
        use super::*;

        fn setup_profiles(state: &mut AppState, count: usize) {
            let names: Vec<String> = (1..=count).map(|i| format!("conn{}", i)).collect();
            let profiles = names.iter().map(|n| create_test_profile(n)).collect();
            state.set_connections(profiles);
        }

        #[test]
        fn select_next_increments_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 3);
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_next_stops_at_last() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_previous_decrements_selection() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 2);
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn select_previous_stops_at_first() {
            let mut state = AppState::new("test".to_string());
            setup_profiles(&mut state, 1);
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }
    }

    mod connections_loaded {
        use super::*;

        #[test]
        fn sorts_connections_by_name_case_insensitive() {
            let mut state = AppState::new("test".to_string());
            let profiles = vec![
                create_test_profile("Zebra"),
                create_test_profile("alpha"),
                create_test_profile("Beta"),
            ];

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.connections()[0].display_name(), "alpha");
            assert_eq!(state.connections()[1].display_name(), "Beta");
            assert_eq!(state.connections()[2].display_name(), "Zebra");
        }

        #[test]
        fn initializes_selection_when_not_empty() {
            let mut state = AppState::new("test".to_string());
            let profiles = vec![create_test_profile("conn1")];

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles,
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn stores_service_file_path_in_runtime() {
            let mut state = AppState::new("test".to_string());
            let path = std::path::PathBuf::from("/etc/pg_service.conf");

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: Some(path.clone()),
                    profile_load_warning: None,
                    service_load_warning: None,
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.runtime.service_file_path, Some(path));
        }

        #[test]
        fn service_load_warning_sets_error_message() {
            let mut state = AppState::new("test".to_string());

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(ConnectionsLoadedPayload {
                    profiles: vec![],
                    services: vec![],
                    service_file_path: None,
                    profile_load_warning: None,
                    service_load_warning: Some("parse error at line 5".to_string()),
                }),
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(state.messages.last_error.is_some());
        }
    }

    mod confirm_connection_selection {
        use super::*;
        use crate::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

        fn create_test_profile_with_id(name: &str, id: ConnectionId) -> ConnectionProfile {
            ConnectionProfile {
                id,
                name: ConnectionName::new(name).unwrap(),
                host: "localhost".to_string(),
                port: 5432,
                database: "test".to_string(),
                username: "user".to_string(),
                password: "pass".to_string(),
                ssl_mode: SslMode::Prefer,
            }
        }

        #[test]
        fn different_connection_dispatches_switch_effect() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.set_connections(vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ]);
            state.session.active_connection_id = Some(active_id);
            state.ui.set_connection_list_selection(Some(1));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            let effects = effects.unwrap();
            assert!(effects.iter().any(
                |e| matches!(e, Effect::SwitchConnection { connection_index } if *connection_index == 1)
            ));
        }

        #[test]
        fn stays_on_same_connection_returns_to_tables() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.set_connections(vec![create_test_profile_with_id(
                "active",
                active_id.clone(),
            )]);
            state.session.active_connection_id = Some(active_id);
            state.ui.set_connection_list_selection(Some(0));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            let effects = effects.unwrap();
            assert!(effects.is_empty());
        }

        #[test]
        fn empty_connections_returns_empty_effects() {
            let mut state = AppState::new("test".to_string());

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            let effects = effects.unwrap();
            assert!(effects.is_empty());
        }

        #[test]
        fn from_selector_mode_switches_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.set_connections(vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ]);
            state.session.active_connection_id = Some(active_id);
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(1));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);

            let effects = effects.unwrap();
            assert!(effects.iter().any(
                |e| matches!(e, Effect::SwitchConnection { connection_index } if *connection_index == 1)
            ));
        }

        #[test]
        fn from_selector_same_connection_returns_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.set_connections(vec![create_test_profile_with_id(
                "active",
                active_id.clone(),
            )]);
            state.session.active_connection_id = Some(active_id);
            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(0));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.input_mode(), InputMode::Normal);

            let effects = effects.unwrap();
            assert!(effects.is_empty());
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

    mod inspector_scroll_top_bottom {
        use super::*;
        use crate::domain::{Column, Table};

        fn state_with_table_detail(columns: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;
            state.ui.inspector_tab = crate::app::inspector_tab::InspectorTab::Columns;
            let cols: Vec<Column> = (0..columns)
                .map(|i| Column {
                    name: format!("col_{}", i),
                    data_type: "text".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: i as i32,
                })
                .collect();
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "test_table".to_string(),
                owner: None,
                columns: cols,
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: Some(0),
                comment: None,
            }));
            state
        }

        #[test]
        fn inspector_scroll_top_resets_to_zero() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 10;

            let effects = reduce_navigation(
                &mut state,
                &Action::InspectorScrollTop,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_scroll_bottom_goes_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 0;
            let visible = state.inspector_visible_rows(); // 10 - 5 = 5
            let expected_max = 20_usize.saturating_sub(visible);

            let effects = reduce_navigation(
                &mut state,
                &Action::InspectorScrollBottom,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, expected_max);
        }

        #[test]
        fn inspector_scroll_bottom_no_detail_stays_zero() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;

            let effects = reduce_navigation(
                &mut state,
                &Action::InspectorScrollBottom,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }
    }

    mod explorer_page_scroll {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};
        use std::sync::Arc;

        fn state_with_tables(count: usize, pane_height: u16) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = pane_height;
            state.ui.focused_pane = FocusedPane::Explorer;
            let tables: Vec<TableSummary> = (0..count)
                .map(|i| {
                    TableSummary::new("public".to_string(), format!("table_{}", i), Some(0), false)
                })
                .collect();
            state.session.set_metadata(Some(Arc::new(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
                fetched_at: Instant::now(),
            })));
            state.ui.set_explorer_selection(Some(0));
            state
        }

        #[test]
        fn half_page_down_jumps_by_correct_delta() {
            let mut state = state_with_tables(50, 23);
            // explorer_visible_items = 23-3 = 20, half = 10
            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 10);
        }

        #[test]
        fn half_page_down_clamped_at_last() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(45));

            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 49);
        }

        #[test]
        fn half_page_up_clamped_at_zero() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(3));

            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageUp,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn full_page_down_jumps_by_visible() {
            let mut state = state_with_tables(50, 23);
            // delta = 20
            reduce_navigation(
                &mut state,
                &Action::SelectFullPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 20);
        }

        #[test]
        fn empty_list_does_nothing() {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = 23;

            let effects = reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn zero_height_pane_scrolls_by_one() {
            let mut state = state_with_tables(50, 0);
            // explorer_visible_items = 0, delta = max(0/2,1) = 1
            reduce_navigation(
                &mut state,
                &Action::SelectHalfPageDown,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 1);
        }

        #[test]
        fn select_middle_moves_to_center() {
            let mut state = state_with_tables(50, 23);
            // visible = 20, len = 50, target = 25
            // scroll_offset = 25 - 10 = 15
            reduce_navigation(
                &mut state,
                &Action::SelectMiddle,
                &AppServices::stub(),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_selected, 25);
            assert_eq!(state.ui.explorer_scroll_offset, 15);
        }
    }
}
