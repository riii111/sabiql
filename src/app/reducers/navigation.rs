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

        // Result Scroll
        Action::ResultScrollUp => {
            state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::ResultScrollDown => {
            let visible = state.result_visible_rows();
            let max_scroll = state
                .query
                .current_result
                .as_ref()
                .map(|r| r.rows.len().saturating_sub(visible))
                .unwrap_or(0);
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
            let visible = state.result_visible_rows();
            let max_scroll = state
                .query
                .current_result
                .as_ref()
                .map(|r| r.rows.len().saturating_sub(visible))
                .unwrap_or(0);
            state.ui.result_scroll_offset = max_scroll;
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
            let visible = match state.ui.inspector_tab {
                InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
                _ => state.inspector_visible_rows(),
            };
            let total_items = state
                .cache
                .table_detail
                .as_ref()
                .map(|t| match state.ui.inspector_tab {
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
                    InspectorTab::Ddl => ddl_line_count_postgres(t),
                })
                .unwrap_or(0);
            let max_offset = total_items.saturating_sub(visible);
            if state.ui.inspector_scroll_offset < max_offset {
                state.ui.inspector_scroll_offset += 1;
            }
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
            if let Some(selected) = state.connections.get(state.ui.connection_list_selected) {
                let selected_id = selected.id.clone();
                let active_id = state.runtime.active_connection_id.clone();

                // Only switch if different from current connection
                if active_id.as_ref() != Some(&selected_id) {
                    let dsn = selected.to_dsn();
                    let name = selected.display_name().to_string();
                    let id = selected_id;

                    // Return to Tables mode after switching
                    state.ui.explorer_mode = ExplorerMode::Tables;

                    return Some(vec![Effect::DispatchActions(vec![
                        Action::SwitchConnection { id, dsn, name },
                    ])]);
                }
            }
            // Already on this connection, just go back to Tables mode
            state.ui.explorer_mode = ExplorerMode::Tables;
            Some(vec![])
        }

        _ => None,
    }
}
