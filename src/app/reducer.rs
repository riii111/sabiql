//! Pure reducer: state transitions only, no I/O.
//!
//! # Purity Rules
//!
//! The reducer MUST NOT:
//! - Call `Instant::now()` (time is passed as `now` parameter)
//! - Perform I/O operations
//! - Spawn async tasks
//!
//! This keeps the reducer testable without mocking time or I/O.

use std::time::{Duration, Instant};

use crate::app::action::{Action, CursorMove};
use crate::app::ddl::ddl_line_count_postgres;
use crate::app::effect::Effect;
use crate::app::focused_pane::FocusedPane;
use crate::app::input_mode::InputMode;
use crate::app::inspector_tab::InspectorTab;
use crate::app::palette::palette_command_count;
use crate::app::state::AppState;
use crate::app::viewport::{calculate_next_column_offset, calculate_prev_column_offset};
use crate::domain::MetadataState;

pub fn reduce(state: &mut AppState, action: Action, now: Instant) -> Vec<Effect> {
    match action {
        Action::None => vec![],
        Action::Quit => {
            state.should_quit = true;
            vec![]
        }
        Action::Resize(_w, h) => {
            state.ui.terminal_height = h;
            vec![]
        }
        Action::Render => {
            state.clear_expired_messages();
            vec![Effect::Render]
        }

        // ===== Focus & Navigation =====
        Action::SetFocusedPane(pane) => {
            state.ui.focused_pane = pane;
            vec![]
        }
        Action::ToggleFocus => {
            state.toggle_focus();
            vec![]
        }
        Action::InspectorNextTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.next();
            vec![]
        }
        Action::InspectorPrevTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.prev();
            vec![]
        }

        // ===== Modal/Overlay Toggles =====
        Action::OpenTablePicker => {
            state.ui.input_mode = InputMode::TablePicker;
            state.ui.filter_input.clear();
            state.ui.picker_selected = 0;
            vec![]
        }
        Action::CloseTablePicker => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }
        Action::OpenCommandPalette => {
            state.ui.input_mode = InputMode::CommandPalette;
            state.ui.picker_selected = 0;
            vec![]
        }
        Action::CloseCommandPalette => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }
        Action::OpenHelp => {
            state.ui.input_mode = if state.ui.input_mode == InputMode::Help {
                InputMode::Normal
            } else {
                InputMode::Help
            };
            vec![]
        }
        Action::CloseHelp => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }
        Action::CloseSqlModal => {
            state.ui.input_mode = InputMode::Normal;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            vec![]
        }
        Action::Escape => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }

        // ===== Connection Modes =====
        Action::OpenConnectionSetup => {
            state.ui.input_mode = InputMode::ConnectionSetup;
            vec![]
        }
        Action::CloseConnectionSetup => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }
        Action::OpenConnectionError => {
            state.ui.input_mode = InputMode::ConnectionError;
            vec![]
        }
        Action::CloseConnectionError => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }
        Action::OpenConfirmDialog => {
            state.ui.input_mode = InputMode::ConfirmDialog;
            vec![]
        }
        Action::CloseConfirmDialog => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }

        // Connection Setup form actions (implemented in Phase C)
        Action::ConnectionSetupInput(_)
        | Action::ConnectionSetupBackspace
        | Action::ConnectionSetupNextField
        | Action::ConnectionSetupPrevField
        | Action::ConnectionSetupToggleDropdown
        | Action::ConnectionSetupDropdownNext
        | Action::ConnectionSetupDropdownPrev
        | Action::ConnectionSetupDropdownConfirm
        | Action::ConnectionSetupDropdownCancel
        | Action::ConnectionSetupSave
        | Action::ConnectionSetupCancel
        | Action::ConnectionSaveCompleted
        | Action::ConnectionSaveFailed(_) => {
            vec![]
        }

        // Confirm Dialog actions (implemented in Phase C)
        Action::ConfirmDialogConfirm | Action::ConfirmDialogCancel => {
            vec![]
        }

        // ===== Filter Input =====
        Action::FilterInput(c) => {
            state.ui.filter_input.push(c);
            state.ui.picker_selected = 0;
            vec![]
        }
        Action::FilterBackspace => {
            state.ui.filter_input.pop();
            state.ui.picker_selected = 0;
            vec![]
        }

        // ===== Command Line =====
        Action::EnterCommandLine => {
            state.ui.input_mode = InputMode::CommandLine;
            state.command_line_input.clear();
            vec![]
        }
        Action::ExitCommandLine => {
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }
        Action::CommandLineInput(c) => {
            state.command_line_input.push(c);
            vec![]
        }
        Action::CommandLineBackspace => {
            state.command_line_input.pop();
            vec![]
        }

        // ===== Selection =====
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
            vec![]
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
            vec![]
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
            vec![]
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
            vec![]
        }

        // ===== Result Scroll =====
        Action::ResultScrollUp => {
            state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(1);
            vec![]
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
            vec![]
        }
        Action::ResultScrollTop => {
            state.ui.result_scroll_offset = 0;
            vec![]
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
            vec![]
        }
        Action::ResultScrollLeft => {
            state.ui.result_horizontal_offset =
                calculate_prev_column_offset(state.ui.result_horizontal_offset);
            vec![]
        }
        Action::ResultScrollRight => {
            let plan = &state.ui.result_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.result_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.result_horizontal_offset,
                plan.column_count,
            );
            vec![]
        }

        // ===== Inspector Scroll =====
        Action::InspectorScrollUp => {
            state.ui.inspector_scroll_offset = state.ui.inspector_scroll_offset.saturating_sub(1);
            vec![]
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
            vec![]
        }
        Action::InspectorScrollLeft => {
            state.ui.inspector_horizontal_offset =
                calculate_prev_column_offset(state.ui.inspector_horizontal_offset);
            vec![]
        }
        Action::InspectorScrollRight => {
            let plan = &state.ui.inspector_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.inspector_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.inspector_horizontal_offset,
                plan.column_count,
            );
            vec![]
        }

        // ===== Explorer Scroll =====
        Action::ExplorerScrollLeft => {
            state.ui.explorer_horizontal_offset =
                state.ui.explorer_horizontal_offset.saturating_sub(1);
            vec![]
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
            vec![]
        }

        // ===== Completion UI =====
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
            vec![]
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
            vec![]
        }
        Action::CompletionDismiss => {
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            vec![]
        }

        // ===== SQL Modal Text Editing =====
        Action::SqlModalInput(c) => {
            state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, c);
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            vec![]
        }
        Action::SqlModalBackspace => {
            state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
            if state.sql_modal.cursor > 0 {
                state.sql_modal.cursor -= 1;
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            vec![]
        }
        Action::SqlModalDelete => {
            state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
            let total_chars = char_count(&state.sql_modal.content);
            if state.sql_modal.cursor < total_chars {
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            vec![]
        }
        Action::SqlModalNewLine => {
            state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, '\n');
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            vec![]
        }
        Action::SqlModalTab => {
            state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert_str(byte_idx, "    ");
            state.sql_modal.cursor += 4;
            state.sql_modal.completion_debounce = Some(now + Duration::from_millis(100));
            vec![]
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
            vec![]
        }
        Action::SqlModalClear => {
            state.sql_modal.content.clear();
            state.sql_modal.cursor = 0;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            vec![]
        }

        // ===== Response Handlers (pure state updates) =====
        Action::MetadataLoaded(metadata) => {
            let has_tables = !metadata.tables.is_empty();
            state.cache.metadata = Some(*metadata);
            state.cache.state = MetadataState::Loaded;
            state
                .ui
                .set_explorer_selection(if has_tables { Some(0) } else { None });

            // If SqlModal is already open and prefetch hasn't started, start it now
            if state.ui.input_mode == InputMode::SqlModal && !state.sql_modal.prefetch_started {
                vec![Effect::DispatchActions(vec![Action::StartPrefetchAll])]
            } else {
                vec![]
            }
        }
        Action::MetadataFailed(error) => {
            state.cache.state = MetadataState::Error(error);
            vec![]
        }
        Action::TableDetailLoaded(detail, generation) => {
            if generation == state.cache.selection_generation {
                state.cache.table_detail = Some(*detail);
                state.ui.inspector_scroll_offset = 0;
            }
            vec![]
        }
        Action::TableDetailFailed(error, generation) => {
            if generation == state.cache.selection_generation {
                state.set_error(error);
            }
            vec![]
        }
        Action::QueryCompleted(result, generation) => {
            if generation == 0 || generation == state.cache.selection_generation {
                state.query.status = crate::app::query_execution::QueryStatus::Idle;
                state.query.start_time = None;
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                state.query.result_highlight_until = Some(now + Duration::from_millis(500));
                state.query.history_index = None;

                if result.source == crate::domain::QuerySource::Adhoc {
                    if result.is_error() {
                        state.sql_modal.status =
                            crate::app::sql_modal_context::SqlModalStatus::Error;
                    } else {
                        state.sql_modal.status =
                            crate::app::sql_modal_context::SqlModalStatus::Success;
                    }
                }

                if result.source == crate::domain::QuerySource::Adhoc && !result.is_error() {
                    state.query.result_history.push((*result).clone());
                }

                state.query.current_result = Some(*result);
            }
            vec![]
        }
        Action::QueryFailed(error, generation) => {
            if generation == 0 || generation == state.cache.selection_generation {
                state.query.status = crate::app::query_execution::QueryStatus::Idle;
                state.query.start_time = None;
                state.set_error(error.clone());
                if state.ui.input_mode == InputMode::SqlModal {
                    state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Error;
                    let error_result = crate::domain::QueryResult::error(
                        state.sql_modal.content.clone(),
                        error,
                        0,
                        crate::domain::QuerySource::Adhoc,
                    );
                    state.query.current_result = Some(error_result);
                }
            }
            vec![]
        }
        Action::ErDiagramOpened {
            path,
            table_count,
            total_tables,
        } => {
            state.er_preparation.status = crate::app::er_state::ErStatus::Idle;
            state.set_success(format!(
                "✓ Opened {} ({}/{} tables)",
                path, table_count, total_tables
            ));
            vec![]
        }
        Action::ErDiagramFailed(error) => {
            state.er_preparation.status = crate::app::er_state::ErStatus::Idle;
            state.set_error(error);
            vec![]
        }

        // ===== Phase 3: Async Actions =====
        Action::OpenSqlModal => {
            state.ui.input_mode = InputMode::SqlModal;
            state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            state.sql_modal.completion.selected_index = 0;
            state.sql_modal.completion_debounce = None;
            // Dispatch StartPrefetchAll if not already started and metadata is loaded
            if !state.sql_modal.prefetch_started && state.cache.metadata.is_some() {
                vec![Effect::DispatchActions(vec![Action::StartPrefetchAll])]
            } else {
                vec![]
            }
        }

        Action::SqlModalSubmit => {
            let query = state.sql_modal.content.trim().to_string();
            if !query.is_empty() {
                state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Running;
                state.sql_modal.completion.visible = false;
                if let Some(dsn) = &state.runtime.dsn {
                    vec![Effect::ExecuteAdhoc {
                        dsn: dsn.clone(),
                        query,
                    }]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        }

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
            vec![]
        }

        Action::CommandLineSubmit => {
            use crate::app::command::{command_to_action, parse_command};

            let cmd = parse_command(&state.command_line_input);
            let follow_up = command_to_action(cmd);
            state.ui.input_mode = InputMode::Normal;
            state.command_line_input.clear();

            match follow_up {
                Action::Quit => {
                    state.should_quit = true;
                    vec![]
                }
                Action::OpenHelp => {
                    state.ui.input_mode = InputMode::Help;
                    vec![]
                }
                Action::OpenSqlModal => {
                    state.ui.input_mode = InputMode::SqlModal;
                    state.sql_modal.status = crate::app::sql_modal_context::SqlModalStatus::Editing;
                    if !state.sql_modal.prefetch_started && state.cache.metadata.is_some() {
                        vec![Effect::DispatchActions(vec![Action::StartPrefetchAll])]
                    } else {
                        vec![]
                    }
                }
                Action::ErOpenDiagram => {
                    // Will be handled in Phase 4
                    vec![]
                }
                _ => vec![],
            }
        }

        Action::LoadMetadata => {
            // Note: Cache check is done in EffectRunner
            if let Some(dsn) = &state.runtime.dsn {
                state.cache.state = MetadataState::Loading;
                vec![Effect::FetchMetadata { dsn: dsn.clone() }]
            } else {
                vec![]
            }
        }

        Action::LoadTableDetail {
            schema,
            table,
            generation,
        } => {
            if let Some(dsn) = &state.runtime.dsn {
                vec![Effect::FetchTableDetail {
                    dsn: dsn.clone(),
                    schema,
                    table,
                    generation,
                }]
            } else {
                vec![]
            }
        }

        Action::ExecutePreview {
            schema,
            table,
            generation,
        } => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = crate::app::query_execution::QueryStatus::Running;
                state.query.start_time = Some(now);

                // Adaptive limit: fewer rows for wide tables to avoid UI lag
                let limit = state.cache.table_detail.as_ref().map_or(100, |detail| {
                    let col_count = detail.columns.len();
                    if col_count >= 30 {
                        20
                    } else if col_count >= 20 {
                        50
                    } else {
                        100
                    }
                });

                vec![Effect::ExecutePreview {
                    dsn: dsn.clone(),
                    schema,
                    table,
                    generation,
                    limit,
                }]
            } else {
                vec![]
            }
        }

        Action::ExecuteAdhoc(query) => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = crate::app::query_execution::QueryStatus::Running;
                state.query.start_time = Some(now);
                vec![Effect::ExecuteAdhoc {
                    dsn: dsn.clone(),
                    query,
                }]
            } else {
                vec![]
            }
        }

        Action::StartPrefetchAll => {
            if !state.sql_modal.prefetch_started
                && let Some(metadata) = &state.cache.metadata
            {
                state.sql_modal.prefetch_started = true;
                state.sql_modal.prefetch_queue.clear();
                state.er_preparation.pending_tables.clear();
                state.er_preparation.fetching_tables.clear();
                state.er_preparation.failed_tables.clear();
                state.er_preparation.total_tables = metadata.tables.len();

                // Queue all tables; EffectRunner skips already-cached ones.
                // Pre-filtering here would require completion_engine access, breaking reducer purity.
                for table_summary in &metadata.tables {
                    let qualified_name = table_summary.qualified_name();
                    state
                        .sql_modal
                        .prefetch_queue
                        .push_back(qualified_name.clone());
                    state.er_preparation.pending_tables.insert(qualified_name);
                }
                vec![Effect::ProcessPrefetchQueue]
            } else {
                vec![]
            }
        }

        Action::ProcessPrefetchQueue => {
            const MAX_CONCURRENT_PREFETCH: usize = 4;
            let current_in_flight = state.sql_modal.prefetching_tables.len();
            let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);

            // Dispatch Action::PrefetchTableDetail for each slot
            // This ensures in-flight management and backoff are applied
            let mut actions = Vec::new();
            for _ in 0..available_slots {
                if let Some(qualified_name) = state.sql_modal.prefetch_queue.pop_front()
                    && let Some((schema, table)) = qualified_name.split_once('.')
                {
                    actions.push(Action::PrefetchTableDetail {
                        schema: schema.to_string(),
                        table: table.to_string(),
                    });
                }
            }

            if actions.is_empty() {
                vec![]
            } else {
                vec![Effect::DispatchActions(actions)]
            }
        }

        Action::ConfirmSelection => {
            let mut effects = Vec::new();

            if state.ui.input_mode == InputMode::TablePicker {
                let filtered = state.filtered_tables();
                if let Some(table) = filtered.get(state.ui.picker_selected).cloned() {
                    let schema = table.schema.clone();
                    let table_name = table.name.clone();
                    state.cache.current_table = Some(table.qualified_name());
                    state.ui.input_mode = InputMode::Normal;

                    state.cache.selection_generation += 1;
                    let current_gen = state.cache.selection_generation;

                    if let Some(dsn) = &state.runtime.dsn {
                        effects.push(Effect::FetchTableDetail {
                            dsn: dsn.clone(),
                            schema: schema.clone(),
                            table: table_name.clone(),
                            generation: current_gen,
                        });
                        effects.push(Effect::ExecutePreview {
                            dsn: dsn.clone(),
                            schema,
                            table: table_name,
                            generation: current_gen,
                            limit: 100,
                        });
                    }
                }
            } else if state.ui.input_mode == InputMode::Normal
                && state.ui.focused_pane == FocusedPane::Explorer
            {
                let tables = state.tables();
                if let Some(table) = tables.get(state.ui.explorer_selected).cloned() {
                    let schema = table.schema.clone();
                    let table_name = table.name.clone();
                    state.cache.current_table = Some(table.qualified_name());

                    state.cache.selection_generation += 1;
                    let current_gen = state.cache.selection_generation;

                    if let Some(dsn) = &state.runtime.dsn {
                        effects.push(Effect::FetchTableDetail {
                            dsn: dsn.clone(),
                            schema: schema.clone(),
                            table: table_name.clone(),
                            generation: current_gen,
                        });
                        effects.push(Effect::ExecutePreview {
                            dsn: dsn.clone(),
                            schema,
                            table: table_name,
                            generation: current_gen,
                            limit: 100,
                        });
                    }
                }
            } else if state.ui.input_mode == InputMode::CommandPalette {
                use crate::app::palette::palette_action_for_index;

                let cmd_action = palette_action_for_index(state.ui.picker_selected);
                state.ui.input_mode = InputMode::Normal;

                match cmd_action {
                    Action::Quit => state.should_quit = true,
                    Action::OpenHelp => state.ui.input_mode = InputMode::Help,
                    Action::OpenTablePicker => {
                        state.ui.input_mode = InputMode::TablePicker;
                        state.ui.filter_input.clear();
                        state.ui.picker_selected = 0;
                    }
                    Action::SetFocusedPane(pane) => state.ui.focused_pane = pane,
                    Action::OpenSqlModal => {
                        state.ui.input_mode = InputMode::SqlModal;
                        state.sql_modal.status =
                            crate::app::sql_modal_context::SqlModalStatus::Editing;
                        if !state.sql_modal.prefetch_started && state.cache.metadata.is_some() {
                            effects.push(Effect::DispatchActions(vec![Action::StartPrefetchAll]));
                        }
                    }
                    Action::ReloadMetadata => {
                        // Will be handled in Phase 4 (needs cache invalidation)
                        if let Some(dsn) = &state.runtime.dsn {
                            effects.push(Effect::Sequence(vec![
                                Effect::CacheInvalidate { dsn: dsn.clone() },
                                Effect::ClearCompletionEngineCache,
                                Effect::FetchMetadata { dsn: dsn.clone() },
                            ]));

                            // Reset prefetch state
                            state.sql_modal.prefetch_started = false;
                            state.sql_modal.prefetch_queue.clear();
                            state.sql_modal.prefetching_tables.clear();
                            state.sql_modal.failed_prefetch_tables.clear();
                            state.er_preparation.reset();
                            state.messages.last_error = None;
                            state.messages.last_success = None;
                            state.messages.expires_at = None;
                        }
                    }
                    _ => {}
                }
            }

            effects
        }

        Action::ReloadMetadata => {
            if let Some(dsn) = &state.runtime.dsn {
                state.sql_modal.prefetch_started = false;
                state.sql_modal.prefetch_queue.clear();
                state.sql_modal.prefetching_tables.clear();
                state.sql_modal.failed_prefetch_tables.clear();
                state.er_preparation.reset();
                state.messages.last_error = None;
                state.messages.last_success = None;
                state.messages.expires_at = None;

                vec![Effect::Sequence(vec![
                    Effect::CacheInvalidate { dsn: dsn.clone() },
                    Effect::ClearCompletionEngineCache,
                    Effect::FetchMetadata { dsn: dsn.clone() },
                ])]
            } else {
                vec![]
            }
        }

        Action::TableDetailCached {
            schema,
            table,
            detail,
        } => {
            use crate::app::er_state::ErStatus;

            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);

            let mut effects = vec![Effect::CacheTableInCompletionEngine {
                qualified_name,
                table: detail,
            }];

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                if !state.er_preparation.has_failures() {
                    state.set_success("ER ready. Press 'e' to open.".to_string());
                } else {
                    let failed_count = state.er_preparation.failed_tables.len();
                    let failed_data: Vec<(String, String)> = state
                        .er_preparation
                        .failed_tables
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    effects.push(Effect::WriteErFailureLog {
                        failed_tables: failed_data,
                    });
                    state.set_error(format!(
                        "ER failed: {} table(s) failed. 'e' to retry.",
                        failed_count
                    ));
                }
            }

            effects
        }

        Action::TableDetailCacheFailed {
            schema,
            table,
            error,
        } => {
            use crate::app::er_state::ErStatus;

            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .insert(qualified_name.clone(), (now, error.clone()));
            state.er_preparation.on_table_failed(&qualified_name, error);

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                let failed_count = state.er_preparation.failed_tables.len();
                let failed_data: Vec<(String, String)> = state
                    .er_preparation
                    .failed_tables
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                effects.push(Effect::WriteErFailureLog {
                    failed_tables: failed_data,
                });
                state.set_error(format!(
                    "ER failed: {} table(s) failed. See log for details. 'e' to retry.",
                    failed_count
                ));
            }

            effects
        }

        Action::TableDetailAlreadyCached { schema, table } => {
            use crate::app::er_state::ErStatus;

            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);

            let mut effects = Vec::new();

            if !state.sql_modal.prefetch_queue.is_empty() {
                effects.push(Effect::ProcessPrefetchQueue);
            }

            // Check if ER preparation is complete (same logic as TableDetailCached)
            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                if !state.er_preparation.has_failures() {
                    state.set_success("ER ready. Press 'e' to open.".to_string());
                } else {
                    let failed_count = state.er_preparation.failed_tables.len();
                    let failed_data: Vec<(String, String)> = state
                        .er_preparation
                        .failed_tables
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    effects.push(Effect::WriteErFailureLog {
                        failed_tables: failed_data,
                    });
                    state.set_error(format!(
                        "ER failed: {} table(s) failed. 'e' to retry.",
                        failed_count
                    ));
                }
            }

            effects
        }

        Action::ErOpenDiagram => {
            use crate::app::er_state::ErStatus;

            if matches!(
                state.er_preparation.status,
                ErStatus::Rendering | ErStatus::Waiting
            ) {
                return vec![];
            }

            // If prefetch hasn't started, start it now and wait
            if !state.sql_modal.prefetch_started
                && let Some(metadata) = &state.cache.metadata
            {
                state.er_preparation.total_tables = metadata.tables.len();
                state.er_preparation.status = ErStatus::Waiting;
                state.set_success("Starting table prefetch for ER diagram...".to_string());
                return vec![Effect::DispatchActions(vec![Action::StartPrefetchAll])];
            }

            // If no metadata yet, show error
            if state.cache.metadata.is_none() {
                state.set_error("Metadata not loaded yet".to_string());
                return vec![];
            }

            if state.er_preparation.has_failures() {
                let failed_tables: Vec<String> =
                    state.er_preparation.failed_tables.keys().cloned().collect();
                state.er_preparation.retry_failed();
                state.sql_modal.failed_prefetch_tables.clear();

                for qualified_name in failed_tables {
                    state.sql_modal.prefetch_queue.push_back(qualified_name);
                }

                state.er_preparation.status = ErStatus::Waiting;
                return vec![Effect::ProcessPrefetchQueue];
            }

            if !state.er_preparation.is_complete() {
                state.er_preparation.status = ErStatus::Waiting;
                return vec![];
            }

            state.er_preparation.status = ErStatus::Rendering;
            let total_tables = state
                .cache
                .metadata
                .as_ref()
                .map(|m| m.tables.len())
                .unwrap_or(0);

            vec![Effect::GenerateErDiagramFromCache {
                total_tables,
                project_name: state.runtime.project_name.clone(),
            }]
        }

        Action::CompletionTrigger => vec![Effect::TriggerCompletion],

        Action::CompletionUpdated {
            candidates,
            trigger_position,
            visible,
        } => {
            state.sql_modal.completion.candidates = candidates;
            state.sql_modal.completion.trigger_position = trigger_position;
            state.sql_modal.completion.selected_index = 0;
            state.sql_modal.completion.visible = visible;
            vec![]
        }

        // PrefetchTableDetail handled in reducer (state update) + EffectRunner (cache check + spawn)
        Action::PrefetchTableDetail { schema, table } => {
            let qualified_name = format!("{}.{}", schema, table);

            // Skip if already in flight
            if state.sql_modal.prefetching_tables.contains(&qualified_name) {
                return vec![];
            }

            // Check backoff for recently failed tables
            const PREFETCH_BACKOFF_SECS: u64 = 30;
            let recently_failed = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified_name)
                .map(|(t, _): &(Instant, String)| t.elapsed().as_secs() < PREFETCH_BACKOFF_SECS)
                .unwrap_or(false);

            if recently_failed {
                return vec![];
            }

            // Mark as in-flight and update ER state
            state
                .sql_modal
                .prefetching_tables
                .insert(qualified_name.clone());
            state.er_preparation.pending_tables.remove(&qualified_name);
            state
                .er_preparation
                .fetching_tables
                .insert(qualified_name.clone());

            if let Some(dsn) = &state.runtime.dsn {
                vec![Effect::PrefetchTableDetail {
                    dsn: dsn.clone(),
                    schema,
                    table,
                }]
            } else {
                vec![]
            }
        }
    }
}

fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

fn char_count(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> AppState {
        AppState::new("test_project".to_string(), "default".to_string())
    }

    mod pure_actions {
        use super::*;
        use rstest::rstest;

        #[test]
        fn quit_sets_should_quit_and_returns_no_effects() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::Quit, now);

            assert!(state.should_quit);
            assert!(effects.is_empty());
        }

        #[test]
        fn toggle_focus_returns_no_effects() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ToggleFocus, now);

            assert!(state.ui.focus_mode);
            assert!(effects.is_empty());
        }

        #[test]
        fn resize_updates_terminal_height() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::Resize(100, 50), now);

            assert_eq!(state.ui.terminal_height, 50);
            assert!(effects.is_empty());
        }

        #[test]
        fn render_returns_render_effect() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::Render, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::Render));
        }

        #[rstest]
        #[case(Action::SelectFirst)]
        #[case(Action::SelectLast)]
        #[case(Action::SelectNext)]
        #[case(Action::SelectPrevious)]
        fn selection_on_empty_tables_keeps_none(#[case] action: Action) {
            let mut state = create_test_state();
            state.ui.focused_pane = FocusedPane::Explorer;
            state.ui.explorer_list_state.select(None);
            let now = Instant::now();

            let _ = reduce(&mut state, action, now);

            assert_eq!(state.ui.explorer_list_state.selected(), None);
        }
    }

    mod scroll_actions {
        use super::*;

        #[test]
        fn result_scroll_up_decrements_offset() {
            let mut state = create_test_state();
            state.ui.result_scroll_offset = 5;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ResultScrollUp, now);

            assert_eq!(state.ui.result_scroll_offset, 4);
            assert!(effects.is_empty());
        }

        #[test]
        fn result_scroll_up_saturates_at_zero() {
            let mut state = create_test_state();
            state.ui.result_scroll_offset = 0;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ResultScrollUp, now);

            assert_eq!(state.ui.result_scroll_offset, 0);
            assert!(effects.is_empty());
        }

        #[test]
        fn result_scroll_top_resets_to_zero() {
            let mut state = create_test_state();
            state.ui.result_scroll_offset = 10;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ResultScrollTop, now);

            assert_eq!(state.ui.result_scroll_offset, 0);
            assert!(effects.is_empty());
        }
    }

    mod modal_toggles {
        use super::*;

        #[test]
        fn open_table_picker_sets_mode_and_clears_filter() {
            let mut state = create_test_state();
            state.ui.filter_input = "test".to_string();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::OpenTablePicker, now);

            assert_eq!(state.ui.input_mode, InputMode::TablePicker);
            assert!(state.ui.filter_input.is_empty());
            assert_eq!(state.ui.picker_selected, 0);
            assert!(effects.is_empty());
        }

        #[test]
        fn close_table_picker_returns_to_normal() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::TablePicker;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CloseTablePicker, now);

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
        }

        #[test]
        fn open_help_toggles_help_mode() {
            let mut state = create_test_state();
            let now = Instant::now();

            // First open
            let effects = reduce(&mut state, Action::OpenHelp, now);
            assert_eq!(state.ui.input_mode, InputMode::Help);
            assert!(effects.is_empty());

            // Toggle back to normal
            let effects = reduce(&mut state, Action::OpenHelp, now);
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert!(effects.is_empty());
        }
    }

    mod sql_modal_debounce {
        use super::*;
        use std::time::Duration;

        #[test]
        fn sql_modal_input_sets_debounce_state() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::SqlModal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::SqlModalInput('a'), now);

            assert_eq!(state.sql_modal.content, "a");
            assert_eq!(state.sql_modal.cursor, 1);
            assert!(effects.is_empty());
            assert!(state.sql_modal.completion_debounce.is_some());
        }

        #[test]
        fn sql_modal_backspace_sets_debounce_state() {
            let mut state = create_test_state();
            state.sql_modal.content = "ab".to_string();
            state.sql_modal.cursor = 2;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::SqlModalBackspace, now);

            assert_eq!(state.sql_modal.content, "a");
            assert_eq!(state.sql_modal.cursor, 1);
            assert!(effects.is_empty());
            assert!(state.sql_modal.completion_debounce.is_some());
        }

        #[test]
        fn debounce_state_uses_provided_now() {
            let mut state = create_test_state();
            let now = Instant::now();

            let _ = reduce(&mut state, Action::SqlModalInput('x'), now);

            let expected = now + Duration::from_millis(100);
            assert_eq!(state.sql_modal.completion_debounce, Some(expected));
        }
    }

    mod completion_ui {
        use super::*;
        use crate::app::sql_modal_context::{CompletionCandidate, CompletionKind};

        fn make_candidate(text: &str) -> CompletionCandidate {
            CompletionCandidate {
                text: text.to_string(),
                kind: CompletionKind::Table,
                score: 0,
            }
        }

        #[test]
        fn completion_next_wraps_around() {
            let mut state = create_test_state();
            state.sql_modal.completion.candidates = vec![make_candidate("a"), make_candidate("b")];
            state.sql_modal.completion.selected_index = 1;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CompletionNext, now);

            assert_eq!(state.sql_modal.completion.selected_index, 0);
            assert!(effects.is_empty());
        }

        #[test]
        fn completion_prev_wraps_around() {
            let mut state = create_test_state();
            state.sql_modal.completion.candidates = vec![make_candidate("a"), make_candidate("b")];
            state.sql_modal.completion.selected_index = 0;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CompletionPrev, now);

            assert_eq!(state.sql_modal.completion.selected_index, 1);
            assert!(effects.is_empty());
        }
    }

    mod response_handlers {
        use super::*;
        use crate::domain::{DatabaseMetadata, TableSummary};

        #[test]
        fn metadata_loaded_with_empty_tables_selects_none() {
            let mut state = create_test_state();
            state.ui.explorer_selected = 5;
            state.ui.explorer_list_state.select(Some(5));
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            };
            let now = Instant::now();

            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            assert!(state.cache.metadata.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
            assert_eq!(state.ui.explorer_list_state.selected(), None);
        }

        #[test]
        fn metadata_loaded_with_tables_selects_first() {
            let mut state = create_test_state();
            state.ui.explorer_selected = 3;
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![TableSummary::new(
                    "public".to_string(),
                    "users".to_string(),
                    None,
                    false,
                )],
                fetched_at: Instant::now(),
            };
            let now = Instant::now();

            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            assert!(state.cache.metadata.is_some());
            assert_eq!(state.ui.explorer_selected, 0);
            assert_eq!(state.ui.explorer_list_state.selected(), Some(0));
        }

        #[test]
        fn metadata_failed_sets_error_state() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::MetadataFailed("Connection failed".to_string()),
                now,
            );

            assert!(matches!(state.cache.state, MetadataState::Error(_)));
            assert!(effects.is_empty());
        }
    }

    mod effect_producing_actions {
        use super::*;

        #[test]
        fn load_metadata_with_dsn_returns_fetch_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::LoadMetadata, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
            assert!(matches!(state.cache.state, MetadataState::Loading));
        }

        #[test]
        fn load_metadata_without_dsn_returns_no_effects() {
            let mut state = create_test_state();
            state.runtime.dsn = None;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::LoadMetadata, now);

            assert!(effects.is_empty());
        }

        #[test]
        fn reload_metadata_returns_sequence_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ReloadMetadata, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::Sequence(_)));

            if let Effect::Sequence(seq) = &effects[0] {
                assert_eq!(seq.len(), 3);
                assert!(matches!(seq[0], Effect::CacheInvalidate { .. }));
                assert!(matches!(seq[1], Effect::ClearCompletionEngineCache));
                assert!(matches!(seq[2], Effect::FetchMetadata { .. }));
            }
        }

        #[test]
        fn execute_adhoc_with_dsn_returns_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ExecuteAdhoc("SELECT 1".to_string()),
                now,
            );

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::ExecuteAdhoc { .. }));
        }
    }

    mod er_diagram {
        use super::*;
        use crate::app::er_state::ErStatus;
        use crate::domain::DatabaseMetadata;

        #[test]
        fn er_open_while_rendering_returns_no_effects() {
            let mut state = create_test_state();
            state.er_preparation.status = ErStatus::Rendering;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert!(effects.is_empty());
        }

        #[test]
        fn er_open_with_incomplete_prefetch_sets_waiting() {
            let mut state = create_test_state();
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });
            state.sql_modal.prefetch_started = true;
            state
                .er_preparation
                .pending_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert!(effects.is_empty());
        }

        #[test]
        fn er_open_when_complete_returns_generate_effect() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });
            state.sql_modal.prefetch_started = true;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(
                effects[0],
                Effect::GenerateErDiagramFromCache { .. }
            ));
            assert_eq!(state.er_preparation.status, ErStatus::Rendering);
        }

        #[test]
        fn er_open_without_prefetch_starts_prefetch() {
            let mut state = create_test_state();
            state.cache.metadata = Some(DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            });
            // prefetch_started is false by default
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert_eq!(state.er_preparation.status, ErStatus::Waiting);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::DispatchActions(_)));
        }

        #[test]
        fn er_open_without_metadata_shows_error() {
            let mut state = create_test_state();
            // No metadata
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ErOpenDiagram, now);

            assert!(state.messages.last_error.is_some());
            assert!(effects.is_empty());
        }
    }

    mod table_detail_cached {
        use super::*;
        use crate::domain::Table;

        fn make_test_table() -> Box<Table> {
            Box::new(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                columns: vec![],
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                row_count_estimate: None,
                comment: None,
            })
        }

        #[test]
        fn table_detail_cached_returns_cache_effect() {
            let mut state = create_test_state();
            state
                .sql_modal
                .prefetching_tables
                .insert("public.users".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::TableDetailCached {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: make_test_table(),
                },
                now,
            );

            assert!(!effects.is_empty());
            assert!(matches!(
                effects[0],
                Effect::CacheTableInCompletionEngine { .. }
            ));
            assert!(!state.sql_modal.prefetching_tables.contains("public.users"));
        }

        #[test]
        fn table_detail_cached_with_queue_returns_process_effect() {
            let mut state = create_test_state();
            state
                .sql_modal
                .prefetch_queue
                .push_back("public.orders".to_string());
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::TableDetailCached {
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: make_test_table(),
                },
                now,
            );

            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::ProcessPrefetchQueue))
            );
        }
    }
}
