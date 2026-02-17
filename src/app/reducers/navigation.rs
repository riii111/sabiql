//! Navigation sub-reducer: focus, scroll, selection, filter, command line.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::ddl::ddl_line_count_postgres;
use crate::app::effect::Effect;
use crate::app::explorer_mode::ExplorerMode;
use crate::app::focused_pane::FocusedPane;
use crate::app::input_mode::InputMode;
use crate::app::inspector_tab::InspectorTab;
use crate::app::palette::palette_command_count;
use crate::app::state::AppState;
use crate::app::viewport::{calculate_next_column_offset, calculate_prev_column_offset};

fn result_max_scroll(state: &AppState) -> usize {
    let visible = state.result_visible_rows();
    state
        .query
        .current_result
        .as_ref()
        .map(|r| r.rows.len().saturating_sub(visible))
        .unwrap_or(0)
}

fn inspector_total_items(state: &AppState) -> usize {
    state
        .cache
        .table_detail
        .as_ref()
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
            InspectorTab::Ddl => ddl_line_count_postgres(t),
        })
        .unwrap_or(0)
}

fn inspector_max_scroll(state: &AppState) -> usize {
    let visible = match state.ui.inspector_tab {
        InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
        _ => state.inspector_visible_rows(),
    };
    inspector_total_items(state).saturating_sub(visible)
}

fn explorer_item_count(state: &AppState) -> usize {
    state.tables().len()
}

/// Handles focus, scroll, selection, filter, and command line actions.
/// Returns Some(effects) if action was handled, None otherwise.
pub fn reduce_navigation(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        Action::SetFocusedPane(pane) => {
            state.ui.focused_pane = *pane;
            Some(vec![])
        }
        Action::ToggleFocus => {
            state.toggle_focus();
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
        Action::Paste(text) => match state.ui.input_mode {
            InputMode::TablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.filter_input.push_str(&clean);
                state.ui.picker_selected = 0;
                Some(vec![])
            }
            InputMode::ErTablePicker => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.ui.er_filter_input.push_str(&clean);
                state.ui.er_picker_selected = 0;
                Some(vec![])
            }
            InputMode::CommandLine => {
                let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                state.command_line_input.push_str(&clean);
                Some(vec![])
            }
            _ => None,
        },

        // Filter
        Action::FilterInput(c) => {
            state.ui.filter_input.push(*c);
            state.ui.picker_selected = 0;
            Some(vec![])
        }
        Action::FilterBackspace => {
            state.ui.filter_input.pop();
            state.ui.picker_selected = 0;
            Some(vec![])
        }

        // Command Line
        Action::EnterCommandLine => {
            state.ui.input_mode = InputMode::CommandLine;
            state.command_line_input.clear();
            Some(vec![])
        }
        Action::ExitCommandLine => {
            state.ui.input_mode = InputMode::Normal;
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
            match state.ui.input_mode {
                InputMode::TablePicker => {
                    let max = state.filtered_tables().len().saturating_sub(1);
                    if state.ui.picker_selected < max {
                        state.ui.picker_selected += 1;
                    }
                }
                InputMode::ErTablePicker => {
                    let max = state.er_filtered_tables().len().saturating_sub(1);
                    if state.ui.er_picker_selected < max {
                        state.ui.er_picker_selected += 1;
                    }
                }
                InputMode::CommandPalette => {
                    let max = palette_command_count() - 1;
                    if state.ui.picker_selected < max {
                        state.ui.picker_selected += 1;
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
            match state.ui.input_mode {
                InputMode::TablePicker | InputMode::CommandPalette => {
                    state.ui.picker_selected = state.ui.picker_selected.saturating_sub(1);
                }
                InputMode::ErTablePicker => {
                    state.ui.er_picker_selected = state.ui.er_picker_selected.saturating_sub(1);
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
            match state.ui.input_mode {
                InputMode::TablePicker | InputMode::CommandPalette => {
                    state.ui.picker_selected = 0;
                }
                InputMode::ErTablePicker => {
                    state.ui.er_picker_selected = 0;
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer && !state.tables().is_empty()
                    {
                        state.ui.set_explorer_selection(Some(0));
                    }
                }
                _ => {}
            }
            Some(vec![])
        }
        Action::SelectLast => {
            match state.ui.input_mode {
                InputMode::TablePicker => {
                    let max = state.filtered_tables().len().saturating_sub(1);
                    state.ui.picker_selected = max;
                }
                InputMode::ErTablePicker => {
                    let max = state.er_filtered_tables().len().saturating_sub(1);
                    state.ui.er_picker_selected = max;
                }
                InputMode::CommandPalette => {
                    state.ui.picker_selected = palette_command_count() - 1;
                }
                InputMode::Normal => {
                    if state.ui.focused_pane == FocusedPane::Explorer {
                        let len = state.tables().len();
                        if len > 0 {
                            state.ui.set_explorer_selection(Some(len - 1));
                        }
                    }
                }
                _ => {}
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

        // Result Scroll
        Action::ResultScrollUp => {
            state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::ResultScrollDown => {
            let max_scroll = result_max_scroll(state);
            if state.ui.result_scroll_offset < max_scroll {
                state.ui.result_scroll_offset += 1;
            }
            Some(vec![])
        }
        Action::ResultScrollTop => {
            state.ui.result_scroll_offset = 0;
            Some(vec![])
        }
        Action::ResultScrollBottom => {
            state.ui.result_scroll_offset = result_max_scroll(state);
            Some(vec![])
        }
        Action::ResultScrollHalfPageDown => {
            let delta = (state.result_visible_rows() / 2).max(1);
            let max = result_max_scroll(state);
            state.ui.result_scroll_offset = (state.ui.result_scroll_offset + delta).min(max);
            Some(vec![])
        }
        Action::ResultScrollHalfPageUp => {
            let delta = (state.result_visible_rows() / 2).max(1);
            state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }
        Action::ResultScrollFullPageDown => {
            let delta = state.result_visible_rows().max(1);
            let max = result_max_scroll(state);
            state.ui.result_scroll_offset = (state.ui.result_scroll_offset + delta).min(max);
            Some(vec![])
        }
        Action::ResultScrollFullPageUp => {
            let delta = state.result_visible_rows().max(1);
            state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(delta);
            Some(vec![])
        }
        Action::ResultScrollLeft => {
            state.ui.result_horizontal_offset =
                calculate_prev_column_offset(state.ui.result_horizontal_offset);
            Some(vec![])
        }
        Action::ResultScrollRight => {
            let plan = &state.ui.result_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.result_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.result_horizontal_offset,
                plan.column_count,
            );
            Some(vec![])
        }

        // Inspector Scroll
        Action::InspectorScrollUp => {
            state.ui.inspector_scroll_offset = state.ui.inspector_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::InspectorScrollDown => {
            let max_offset = inspector_max_scroll(state);
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
            state.ui.inspector_scroll_offset = inspector_max_scroll(state);
            Some(vec![])
        }
        Action::InspectorScrollHalfPageDown => {
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let delta = (visible / 2).max(1);
            let max = inspector_max_scroll(state);
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
            let max = inspector_max_scroll(state);
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

        // Explorer Mode (Tables / Connections)
        Action::ToggleExplorerMode => {
            match state.ui.explorer_mode {
                ExplorerMode::Tables => {
                    state.ui.explorer_mode = ExplorerMode::Connections;
                    // Initialize selection if needed
                    if !state.connections.is_empty() && state.ui.connection_list_selected == 0 {
                        state.ui.set_connection_list_selection(Some(0));
                    }
                    // Load connections list
                    Some(vec![Effect::LoadConnections])
                }
                ExplorerMode::Connections => {
                    state.ui.explorer_mode = ExplorerMode::Tables;
                    Some(vec![])
                }
            }
        }
        Action::SetExplorerMode(mode) => {
            state.ui.explorer_mode = *mode;
            if *mode == ExplorerMode::Connections {
                Some(vec![Effect::LoadConnections])
            } else {
                Some(vec![])
            }
        }
        Action::ConnectionListSelectNext => {
            let len = state.connections.len();
            if len > 0 && state.ui.connection_list_selected < len - 1 {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected + 1));
            }
            Some(vec![])
        }
        Action::ConnectionListSelectPrevious => {
            if !state.connections.is_empty() {
                let new_idx = state.ui.connection_list_selected.saturating_sub(1);
                state.ui.set_connection_list_selection(Some(new_idx));
            }
            Some(vec![])
        }
        Action::ConnectionsLoaded(profiles) => {
            // Sort by name (case-insensitive)
            let mut sorted = profiles.clone();
            sorted.sort_by(|a, b| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            });
            state.connections = sorted;
            // Reset selection if out of bounds
            if state.ui.connection_list_selected >= state.connections.len() {
                let new_idx = state.connections.len().saturating_sub(1);
                state.ui.set_connection_list_selection(Some(new_idx));
            } else if !state.connections.is_empty() {
                state
                    .ui
                    .set_connection_list_selection(Some(state.ui.connection_list_selected));
            }
            Some(vec![])
        }
        Action::ConfirmConnectionSelection => {
            let from_selector = state.ui.input_mode == InputMode::ConnectionSelector;

            if let Some(selected) = state.connections.get(state.ui.connection_list_selected) {
                let selected_id = selected.id.clone();
                let active_id = state.runtime.active_connection_id.clone();

                // Only switch if different from current connection
                if active_id.as_ref() != Some(&selected_id) {
                    let dsn = selected.to_dsn();
                    let name = selected.display_name().to_string();
                    let id = selected_id;

                    // Return to Tables mode and Normal input mode after switching
                    state.ui.explorer_mode = ExplorerMode::Tables;
                    if from_selector {
                        state.ui.input_mode = InputMode::Normal;
                    }

                    return Some(vec![Effect::DispatchActions(vec![
                        Action::SwitchConnection { id, dsn, name },
                    ])]);
                }
            }
            // Already on this connection, just go back to Tables mode / Normal
            state.ui.explorer_mode = ExplorerMode::Tables;
            if from_selector {
                state.ui.input_mode = InputMode::Normal;
            }
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::Effect;
    use std::time::Instant;

    mod paste {
        use super::*;

        #[test]
        fn paste_in_table_picker_appends_text() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::TablePicker;

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("hello".to_string()),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.filter_input, "hello");
        }

        #[test]
        fn paste_in_table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::TablePicker;

            reduce_navigation(
                &mut state,
                &Action::Paste("hel\nlo\r\n".to_string()),
                Instant::now(),
            );

            assert_eq!(state.ui.filter_input, "hello");
        }

        #[test]
        fn paste_in_table_picker_resets_selection() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::TablePicker;
            state.ui.picker_selected = 5;

            reduce_navigation(&mut state, &Action::Paste("x".to_string()), Instant::now());

            assert_eq!(state.ui.picker_selected, 0);
        }

        #[test]
        fn paste_in_command_line_appends_text() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::CommandLine;

            reduce_navigation(
                &mut state,
                &Action::Paste("quit".to_string()),
                Instant::now(),
            );

            assert_eq!(state.command_line_input, "quit");
        }

        #[test]
        fn paste_in_command_line_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::CommandLine;

            reduce_navigation(
                &mut state,
                &Action::Paste("qu\nit".to_string()),
                Instant::now(),
            );

            assert_eq!(state.command_line_input, "quit");
        }

        #[test]
        fn paste_in_normal_mode_returns_none() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::Normal;

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("text".to_string()),
                Instant::now(),
            );

            assert!(effects.is_none());
        }

        #[test]
        fn paste_in_er_table_picker_appends_to_er_filter() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::ErTablePicker;

            let effects = reduce_navigation(
                &mut state,
                &Action::Paste("public.users".to_string()),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.er_filter_input, "public.users");
            assert_eq!(state.ui.er_picker_selected, 0);
        }

        #[test]
        fn paste_in_er_table_picker_strips_newlines() {
            let mut state = AppState::new("test".to_string());
            state.ui.input_mode = InputMode::ErTablePicker;

            reduce_navigation(
                &mut state,
                &Action::Paste("public\n.users\r\n".to_string()),
                Instant::now(),
            );

            assert_eq!(state.ui.er_filter_input, "public.users");
        }
    }

    mod explorer_mode {
        use super::*;

        #[test]
        fn toggle_from_tables_switches_to_connections() {
            let mut state = AppState::new("test".to_string());
            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);

            let effects =
                reduce_navigation(&mut state, &Action::ToggleExplorerMode, Instant::now());

            assert_eq!(state.ui.explorer_mode, ExplorerMode::Connections);
            assert!(effects.is_some());
            let effects = effects.unwrap();
            assert!(effects.iter().any(|e| matches!(e, Effect::LoadConnections)));
        }

        #[test]
        fn toggle_from_connections_switches_to_tables() {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_mode = ExplorerMode::Connections;

            let effects =
                reduce_navigation(&mut state, &Action::ToggleExplorerMode, Instant::now());

            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);
            assert!(effects.is_some());
        }

        #[test]
        fn set_explorer_mode_to_connections_loads_connections() {
            let mut state = AppState::new("test".to_string());

            let effects = reduce_navigation(
                &mut state,
                &Action::SetExplorerMode(ExplorerMode::Connections),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_mode, ExplorerMode::Connections);
            let effects = effects.unwrap();
            assert!(effects.iter().any(|e| matches!(e, Effect::LoadConnections)));
        }

        #[test]
        fn set_explorer_mode_to_tables_does_not_load_connections() {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_mode = ExplorerMode::Connections;

            let effects = reduce_navigation(
                &mut state,
                &Action::SetExplorerMode(ExplorerMode::Tables),
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);
            let effects = effects.unwrap();
            assert!(effects.is_empty());
        }
    }

    mod connection_list_navigation {
        use super::*;
        use crate::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

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

        #[test]
        fn select_next_increments_selection() {
            let mut state = AppState::new("test".to_string());
            state.connections = vec![
                create_test_profile("conn1"),
                create_test_profile("conn2"),
                create_test_profile("conn3"),
            ];
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_next_stops_at_last() {
            let mut state = AppState::new("test".to_string());
            state.connections = vec![create_test_profile("conn1"), create_test_profile("conn2")];
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectNext,
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 1);
        }

        #[test]
        fn select_previous_decrements_selection() {
            let mut state = AppState::new("test".to_string());
            state.connections = vec![create_test_profile("conn1"), create_test_profile("conn2")];
            state.ui.set_connection_list_selection(Some(1));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }

        #[test]
        fn select_previous_stops_at_first() {
            let mut state = AppState::new("test".to_string());
            state.connections = vec![create_test_profile("conn1")];
            state.ui.set_connection_list_selection(Some(0));

            reduce_navigation(
                &mut state,
                &Action::ConnectionListSelectPrevious,
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
        }
    }

    mod connections_loaded {
        use super::*;
        use crate::domain::connection::{ConnectionId, ConnectionName, ConnectionProfile, SslMode};

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
                &Action::ConnectionsLoaded(profiles),
                Instant::now(),
            );

            assert_eq!(state.connections[0].display_name(), "alpha");
            assert_eq!(state.connections[1].display_name(), "Beta");
            assert_eq!(state.connections[2].display_name(), "Zebra");
        }

        #[test]
        fn initializes_selection_when_not_empty() {
            let mut state = AppState::new("test".to_string());
            let profiles = vec![create_test_profile("conn1")];

            reduce_navigation(
                &mut state,
                &Action::ConnectionsLoaded(profiles),
                Instant::now(),
            );

            assert_eq!(state.ui.connection_list_selected, 0);
            assert_eq!(state.ui.connection_list_state.selected(), Some(0));
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
        fn different_connection_dispatches_switch_action() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.connections = vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ];
            state.runtime.active_connection_id = Some(active_id);
            state.ui.explorer_mode = ExplorerMode::Connections;
            state.ui.set_connection_list_selection(Some(1)); // Select "other"

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);
            let effects = effects.unwrap();
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::DispatchActions(_)))
            );
        }

        #[test]
        fn stays_on_same_connection_returns_to_tables() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.connections = vec![create_test_profile_with_id("active", active_id.clone())];
            state.runtime.active_connection_id = Some(active_id);
            state.ui.explorer_mode = ExplorerMode::Connections;
            state.ui.set_connection_list_selection(Some(0)); // Select same connection

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);
            let effects = effects.unwrap();
            assert!(effects.is_empty()); // No SwitchConnection dispatched
        }

        #[test]
        fn empty_connections_returns_to_tables() {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_mode = ExplorerMode::Connections;

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);
            let effects = effects.unwrap();
            assert!(effects.is_empty());
        }

        #[test]
        fn from_selector_mode_switches_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();
            let other_id = ConnectionId::new();

            state.connections = vec![
                create_test_profile_with_id("active", active_id.clone()),
                create_test_profile_with_id("other", other_id.clone()),
            ];
            state.runtime.active_connection_id = Some(active_id);
            state.ui.input_mode = InputMode::ConnectionSelector;
            state.ui.set_connection_list_selection(Some(1));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);
            let effects = effects.unwrap();
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::DispatchActions(_)))
            );
        }

        #[test]
        fn from_selector_same_connection_returns_to_normal() {
            let mut state = AppState::new("test".to_string());
            let active_id = ConnectionId::new();

            state.connections = vec![create_test_profile_with_id("active", active_id.clone())];
            state.runtime.active_connection_id = Some(active_id);
            state.ui.input_mode = InputMode::ConnectionSelector;
            state.ui.set_connection_list_selection(Some(0));

            let effects = reduce_navigation(
                &mut state,
                &Action::ConfirmConnectionSelection,
                Instant::now(),
            );

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(state.ui.explorer_mode, ExplorerMode::Tables);
            let effects = effects.unwrap();
            assert!(effects.is_empty());
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
            state.cache.table_detail = Some(Table {
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
            });
            state
        }

        #[test]
        fn inspector_scroll_top_resets_to_zero() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 10;

            let effects =
                reduce_navigation(&mut state, &Action::InspectorScrollTop, Instant::now());

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_scroll_bottom_goes_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 0;
            let visible = state.inspector_visible_rows(); // 10 - 5 = 5
            let expected_max = 20_usize.saturating_sub(visible);

            let effects =
                reduce_navigation(&mut state, &Action::InspectorScrollBottom, Instant::now());

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, expected_max);
        }

        #[test]
        fn inspector_scroll_bottom_no_detail_stays_zero() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;

            let effects =
                reduce_navigation(&mut state, &Action::InspectorScrollBottom, Instant::now());

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }
    }

    mod result_page_scroll {
        use super::*;
        use std::sync::Arc;

        fn state_with_result_rows(rows: usize, pane_height: u16) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.result_pane_height = pane_height;
            let result_rows: Vec<Vec<String>> = (0..rows).map(|i| vec![format!("{}", i)]).collect();
            let row_count = result_rows.len();
            state.query.current_result = Some(Arc::new(crate::domain::QueryResult {
                query: String::new(),
                columns: vec!["id".to_string()],
                rows: result_rows,
                row_count,
                execution_time_ms: 1,
                executed_at: Instant::now(),
                source: crate::domain::QuerySource::Preview,
                error: None,
            }));
            state
        }

        #[test]
        fn half_page_down_from_top() {
            let mut state = state_with_result_rows(100, 25);
            // visible = 25 - 5 = 20, half = 10
            let effects = reduce_navigation(
                &mut state,
                &Action::ResultScrollHalfPageDown,
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.result_scroll_offset, 10);
        }

        #[test]
        fn half_page_up_from_middle() {
            let mut state = state_with_result_rows(100, 25);
            state.ui.result_scroll_offset = 50;

            reduce_navigation(&mut state, &Action::ResultScrollHalfPageUp, Instant::now());

            assert_eq!(state.ui.result_scroll_offset, 40);
        }

        #[test]
        fn full_page_down_clamped_at_max() {
            let mut state = state_with_result_rows(30, 25);
            // visible = 20, max_scroll = 30-20 = 10
            state.ui.result_scroll_offset = 5;

            reduce_navigation(
                &mut state,
                &Action::ResultScrollFullPageDown,
                Instant::now(),
            );

            // delta=20, 5+20=25, clamped to 10
            assert_eq!(state.ui.result_scroll_offset, 10);
        }

        #[test]
        fn full_page_up_clamped_at_zero() {
            let mut state = state_with_result_rows(100, 25);
            state.ui.result_scroll_offset = 5;

            reduce_navigation(&mut state, &Action::ResultScrollFullPageUp, Instant::now());

            // delta=20, saturating_sub(5,20) = 0
            assert_eq!(state.ui.result_scroll_offset, 0);
        }

        #[test]
        fn zero_height_pane_scrolls_by_one() {
            let mut state = state_with_result_rows(100, 0);
            // visible = 0, delta = max(0/2,1) = 1
            reduce_navigation(
                &mut state,
                &Action::ResultScrollHalfPageDown,
                Instant::now(),
            );

            assert_eq!(state.ui.result_scroll_offset, 1);
        }
    }

    mod explorer_page_scroll {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        fn state_with_tables(count: usize, pane_height: u16) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = pane_height;
            state.ui.focused_pane = FocusedPane::Explorer;
            let tables: Vec<TableSummary> = (0..count)
                .map(|i| {
                    TableSummary::new("public".to_string(), format!("table_{}", i), Some(0), false)
                })
                .collect();
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables,
                fetched_at: Instant::now(),
            });
            state.ui.set_explorer_selection(Some(0));
            state
        }

        #[test]
        fn half_page_down_jumps_by_correct_delta() {
            let mut state = state_with_tables(50, 23);
            // explorer_visible_items = 23-3 = 20, half = 10
            reduce_navigation(&mut state, &Action::SelectHalfPageDown, Instant::now());

            assert_eq!(state.ui.explorer_selected, 10);
        }

        #[test]
        fn half_page_down_clamped_at_last() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(45));

            reduce_navigation(&mut state, &Action::SelectHalfPageDown, Instant::now());

            assert_eq!(state.ui.explorer_selected, 49);
        }

        #[test]
        fn half_page_up_clamped_at_zero() {
            let mut state = state_with_tables(50, 23);
            state.ui.set_explorer_selection(Some(3));

            reduce_navigation(&mut state, &Action::SelectHalfPageUp, Instant::now());

            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn full_page_down_jumps_by_visible() {
            let mut state = state_with_tables(50, 23);
            // delta = 20
            reduce_navigation(&mut state, &Action::SelectFullPageDown, Instant::now());

            assert_eq!(state.ui.explorer_selected, 20);
        }

        #[test]
        fn empty_list_does_nothing() {
            let mut state = AppState::new("test".to_string());
            state.ui.explorer_pane_height = 23;

            let effects =
                reduce_navigation(&mut state, &Action::SelectHalfPageDown, Instant::now());

            assert!(effects.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
        }

        #[test]
        fn zero_height_pane_scrolls_by_one() {
            let mut state = state_with_tables(50, 0);
            // explorer_visible_items = 0, delta = max(0/2,1) = 1
            reduce_navigation(&mut state, &Action::SelectHalfPageDown, Instant::now());

            assert_eq!(state.ui.explorer_selected, 1);
        }
    }
}
