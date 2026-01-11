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
use crate::app::connection_error::ConnectionErrorInfo;
use crate::app::connection_setup_state::{CONNECTION_INPUT_VISIBLE_WIDTH, ConnectionField};
use crate::app::connection_state::ConnectionState;
use crate::app::ddl::ddl_line_count_postgres;
use crate::app::effect::Effect;
use crate::app::focused_pane::FocusedPane;
use crate::app::input_mode::InputMode;
use crate::app::inspector_tab::InspectorTab;
use crate::app::palette::palette_command_count;
use crate::app::state::AppState;
use crate::app::viewport::{calculate_next_column_offset, calculate_prev_column_offset};
use crate::domain::MetadataState;
use crate::domain::connection::SslMode;

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

        // ===== Connection Lifecycle =====
        Action::TryConnect => {
            // Idempotency: only connect if NotConnected and in Normal mode
            if state.runtime.connection_state.is_not_connected()
                && state.ui.input_mode == InputMode::Normal
            {
                if let Some(dsn) = state.runtime.dsn.clone() {
                    state.runtime.connection_state = ConnectionState::Connecting;
                    state.cache.state = MetadataState::Loading;
                    vec![Effect::FetchMetadata { dsn }]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
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
        Action::ShowConnectionError(info) => {
            state.connection_error.set_error(info);
            state.ui.input_mode = InputMode::ConnectionError;
            vec![]
        }
        Action::CloseConnectionError => {
            // Keep error_info so Enter can re-open modal
            state.connection_error.details_expanded = false;
            state.connection_error.scroll_offset = 0;
            state.connection_error.clear_copied_feedback();
            state.ui.input_mode = InputMode::Normal;
            vec![]
        }
        Action::ToggleConnectionErrorDetails => {
            state.connection_error.toggle_details();
            vec![]
        }
        Action::ScrollConnectionErrorUp => {
            state.connection_error.scroll_up();
            vec![]
        }
        Action::ScrollConnectionErrorDown => {
            // TODO: Calculate max_scroll from UI viewport in Phase 6b
            state.connection_error.scroll_down(100);
            vec![]
        }
        Action::CopyConnectionError => {
            if let Some(content) = state.connection_error.masked_details() {
                vec![Effect::CopyToClipboard {
                    content: content.to_string(),
                }]
            } else {
                vec![]
            }
        }
        Action::ConnectionErrorCopied => {
            state.connection_error.mark_copied_at(now);
            vec![]
        }
        Action::RetryConnection => {
            if state.runtime.connection_state.is_connecting() || state.runtime.is_reconnecting {
                return vec![];
            }
            if let Some(dsn) = state.runtime.dsn.clone() {
                state.runtime.is_reconnecting = true;
                state.connection_error.is_retrying = true;
                state.runtime.connection_state = ConnectionState::Connecting;
                state.cache.state = MetadataState::Loading;
                vec![Effect::FetchMetadata { dsn }]
            } else {
                state.runtime.connection_state = ConnectionState::NotConnected;
                state.cache.state = MetadataState::NotLoaded;
                state.ui.input_mode = InputMode::ConnectionSetup;
                vec![]
            }
        }
        Action::ReenterConnectionSetup => {
            state.connection_error.clear();
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.cache.state = MetadataState::NotLoaded;
            state.ui.input_mode = InputMode::ConnectionSetup;
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

        // ===== Connection Setup Form =====
        // Note: cursor_position is character-based (not byte-based) for multi-byte safety
        Action::ConnectionSetupInput(c) => {
            let setup = &mut state.connection_setup;
            match setup.focused_field {
                ConnectionField::Host => {
                    insert_char_at_cursor(&mut setup.host, setup.cursor_position, c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::Port => {
                    if c.is_ascii_digit() && setup.port.chars().count() < 5 {
                        insert_char_at_cursor(&mut setup.port, setup.cursor_position, c);
                        let new_cursor = setup.cursor_position + 1;
                        setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                    }
                }
                ConnectionField::Database => {
                    insert_char_at_cursor(&mut setup.database, setup.cursor_position, c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::User => {
                    insert_char_at_cursor(&mut setup.user, setup.cursor_position, c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::Password => {
                    insert_char_at_cursor(&mut setup.password, setup.cursor_position, c);
                    let new_cursor = setup.cursor_position + 1;
                    setup.update_cursor(new_cursor, CONNECTION_INPUT_VISIBLE_WIDTH);
                }
                ConnectionField::SslMode => {}
            }
            vec![]
        }
        Action::ConnectionSetupBackspace => {
            let setup = &mut state.connection_setup;
            if setup.cursor_position == 0 {
                return vec![];
            }
            let field_str = match setup.focused_field {
                ConnectionField::Host => &mut setup.host,
                ConnectionField::Port => &mut setup.port,
                ConnectionField::Database => &mut setup.database,
                ConnectionField::User => &mut setup.user,
                ConnectionField::Password => &mut setup.password,
                ConnectionField::SslMode => return vec![],
            };
            let char_pos = setup.cursor_position - 1;
            if let Some((byte_idx, _)) = field_str.char_indices().nth(char_pos) {
                field_str.remove(byte_idx);
                setup.update_cursor(char_pos, CONNECTION_INPUT_VISIBLE_WIDTH);
            }
            vec![]
        }
        Action::ConnectionSetupNextField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field);
            if let Some(next) = setup.focused_field.next() {
                setup.focused_field = next;
                setup.cursor_to_end();
            }
            vec![]
        }
        Action::ConnectionSetupPrevField => {
            let setup = &mut state.connection_setup;
            validate_field(setup, setup.focused_field);
            if let Some(prev) = setup.focused_field.prev() {
                setup.focused_field = prev;
                setup.cursor_to_end();
            }
            vec![]
        }
        Action::ConnectionSetupToggleDropdown => {
            let setup = &mut state.connection_setup;
            if setup.focused_field == ConnectionField::SslMode {
                setup.ssl_dropdown.is_open = !setup.ssl_dropdown.is_open;
                if setup.ssl_dropdown.is_open {
                    setup.ssl_dropdown.selected_index = SslMode::all_variants()
                        .iter()
                        .position(|v| *v == setup.ssl_mode)
                        .unwrap_or(2);
                }
            }
            vec![]
        }
        Action::ConnectionSetupDropdownNext => {
            let setup = &mut state.connection_setup;
            if setup.ssl_dropdown.is_open {
                let max = SslMode::all_variants().len() - 1;
                if setup.ssl_dropdown.selected_index < max {
                    setup.ssl_dropdown.selected_index += 1;
                }
            }
            vec![]
        }
        Action::ConnectionSetupDropdownPrev => {
            let setup = &mut state.connection_setup;
            if setup.ssl_dropdown.is_open {
                setup.ssl_dropdown.selected_index =
                    setup.ssl_dropdown.selected_index.saturating_sub(1);
            }
            vec![]
        }
        Action::ConnectionSetupDropdownConfirm => {
            let setup = &mut state.connection_setup;
            if setup.ssl_dropdown.is_open {
                if let Some(mode) = SslMode::all_variants().get(setup.ssl_dropdown.selected_index) {
                    setup.ssl_mode = *mode;
                }
                setup.ssl_dropdown.is_open = false;
            }
            vec![]
        }
        Action::ConnectionSetupDropdownCancel => {
            state.connection_setup.ssl_dropdown.is_open = false;
            vec![]
        }
        Action::ConnectionSetupSave => {
            let setup = &mut state.connection_setup;
            validate_all(setup);
            if setup.validation_errors.is_empty() {
                let port = setup.port.parse().unwrap_or(5432);
                vec![Effect::SaveAndConnect {
                    host: setup.host.clone(),
                    port,
                    database: setup.database.clone(),
                    user: setup.user.clone(),
                    password: setup.password.clone(),
                    ssl_mode: setup.ssl_mode,
                }]
            } else {
                vec![]
            }
        }
        Action::ConnectionSetupCancel => {
            if state.connection_setup.is_first_run {
                state.confirm_dialog.title = "Confirm".to_string();
                state.confirm_dialog.message =
                    "No connection configured.\nAre you sure you want to quit?".to_string();
                state.confirm_dialog.on_confirm = Action::Quit;
                state.confirm_dialog.on_cancel = Action::OpenConnectionSetup;
                state.ui.input_mode = InputMode::ConfirmDialog;
                vec![]
            } else {
                state.ui.input_mode = InputMode::Normal;
                vec![Effect::DispatchActions(vec![Action::TryConnect])]
            }
        }
        Action::ConnectionSaveCompleted { dsn } => {
            state.connection_setup.is_first_run = false;
            state.runtime.dsn = Some(dsn.clone());
            state.runtime.active_connection_name = Some(state.connection_setup.auto_name());
            state.runtime.connection_state = ConnectionState::Connecting;
            state.cache.state = MetadataState::Loading;
            state.ui.input_mode = InputMode::Normal;
            vec![Effect::FetchMetadata { dsn }]
        }
        Action::ConnectionSaveFailed(msg) => {
            state.messages.set_error_at(msg, now);
            vec![]
        }

        // ===== Confirm Dialog =====
        Action::ConfirmDialogConfirm => {
            let action = std::mem::replace(&mut state.confirm_dialog.on_confirm, Action::None);
            state.confirm_dialog.on_cancel = Action::None;
            state.ui.input_mode = InputMode::Normal;
            reduce(state, action, now)
        }
        Action::ConfirmDialogCancel => {
            let action = std::mem::replace(&mut state.confirm_dialog.on_cancel, Action::None);
            state.confirm_dialog.on_confirm = Action::None;
            state.ui.input_mode = InputMode::Normal;
            reduce(state, action, now)
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
            state.runtime.connection_state = ConnectionState::Connected;
            state
                .ui
                .set_explorer_selection(if has_tables { Some(0) } else { None });

            let mut effects = vec![];

            state.connection_error.clear();

            if state.runtime.is_reconnecting {
                state.ui.input_mode = InputMode::Normal;
                state
                    .messages
                    .set_success_at("Reconnected!".to_string(), now);
                state.runtime.is_reconnecting = false;
            } else if state.runtime.is_reloading {
                state
                    .messages
                    .set_success_at("Reloaded!".to_string(), now);
                state.runtime.is_reloading = false;
            }

            // If SqlModal is already open and prefetch hasn't started, start it now
            if state.ui.input_mode == InputMode::SqlModal && !state.sql_modal.prefetch_started {
                effects.push(Effect::DispatchActions(vec![Action::StartPrefetchAll]));
            }

            effects
        }
        Action::MetadataFailed(error) => {
            let error_info = ConnectionErrorInfo::new(&error);
            state.connection_error.set_error(error_info);
            state.cache.state = MetadataState::Error(error);
            state.runtime.is_reconnecting = false;
            state.runtime.is_reloading = false;
            // Only set Failed if not already Connected (preserve connection on metadata-only failures)
            // This handles the case where connection succeeds but metadata reload fails
            if !state.runtime.connection_state.is_connected() {
                state.runtime.connection_state = ConnectionState::Failed;
                state.ui.input_mode = InputMode::ConnectionError;
            }
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
            } else if state.ui.input_mode == InputMode::Normal {
                // Open error modal if connection error exists (from any pane)
                if state.connection_error.error_info.is_some() {
                    state.ui.input_mode = InputMode::ConnectionError;
                    return effects;
                }

                if state.ui.focused_pane != FocusedPane::Explorer {
                    return effects;
                }

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
                state.runtime.is_reloading = true;
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

fn insert_char_at_cursor(s: &mut String, char_pos: usize, c: char) {
    let byte_idx = char_to_byte_index(s, char_pos);
    s.insert(byte_idx, c);
}

// ===== Connection Setup Validation =====

use crate::app::connection_setup_state::ConnectionSetupState;

fn validate_field(state: &mut ConnectionSetupState, field: ConnectionField) {
    state.validation_errors.remove(&field);

    match field {
        ConnectionField::Host => {
            if state.host.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::Port => {
            if state.port.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            } else {
                match state.port.parse::<u16>() {
                    Err(_) => {
                        state
                            .validation_errors
                            .insert(field, "Invalid port".to_string());
                    }
                    Ok(0) => {
                        state
                            .validation_errors
                            .insert(field, "Port must be > 0".to_string());
                    }
                    Ok(_) => {}
                }
            }
        }
        ConnectionField::Database => {
            if state.database.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::User => {
            if state.user.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::Password | ConnectionField::SslMode => {
            // Optional fields, no validation needed
        }
    }
}

fn validate_all(state: &mut ConnectionSetupState) {
    for field in ConnectionField::all() {
        validate_field(state, *field);
    }
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
        fn metadata_failed_opens_error_modal_automatically() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::MetadataFailed("psql: error: connection refused".to_string()),
                now,
            );

            assert!(matches!(state.cache.state, MetadataState::Error(_)));
            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
            assert!(state.connection_error.error_info.is_some());
            assert!(effects.is_empty());
        }

        #[test]
        fn enter_with_error_info_opens_modal() {
            let mut state = create_test_state();
            state
                .connection_error
                .set_error(ConnectionErrorInfo::new("error"));
            state.ui.focused_pane = FocusedPane::Result; // Any pane works
            let now = Instant::now();

            reduce(&mut state, Action::ConfirmSelection, now);

            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
        }
    }

    mod connection_error_actions {
        use super::*;
        use crate::app::connection_error::{ConnectionErrorInfo, ConnectionErrorKind};

        fn state_with_error() -> AppState {
            let mut state = create_test_state();
            let info = ConnectionErrorInfo::with_kind(
                ConnectionErrorKind::HostUnreachable,
                "psql: error: could not translate host",
            );
            state.connection_error.set_error(info);
            state.ui.input_mode = InputMode::ConnectionError;
            state
        }

        #[test]
        fn close_keeps_error_info_for_reopen() {
            let mut state = state_with_error();
            state.connection_error.details_expanded = true;
            state.connection_error.scroll_offset = 5;
            let now = Instant::now();

            reduce(&mut state, Action::CloseConnectionError, now);

            // error_info is kept so Enter can re-open modal
            assert!(state.connection_error.error_info.is_some());
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            // UI state is reset
            assert!(!state.connection_error.details_expanded);
            assert_eq!(state.connection_error.scroll_offset, 0);
        }

        #[test]
        fn close_clears_copied_feedback() {
            let mut state = state_with_error();
            let now = Instant::now();
            state.connection_error.mark_copied_at(now);
            assert!(state.connection_error.is_copied_visible_at(now));

            reduce(&mut state, Action::CloseConnectionError, now);

            // Copied feedback is cleared on close
            assert!(!state.connection_error.is_copied_visible_at(now));
        }

        #[test]
        fn reopen_modal_after_close_shows_same_error() {
            let mut state = state_with_error();
            state.cache.state = MetadataState::Error("error".to_string());
            state.ui.focused_pane = FocusedPane::Explorer;
            let now = Instant::now();

            // Close modal
            reduce(&mut state, Action::CloseConnectionError, now);
            assert_eq!(state.ui.input_mode, InputMode::Normal);

            // Re-open with Enter
            reduce(&mut state, Action::ConfirmSelection, now);
            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
            assert!(state.connection_error.error_info.is_some());
        }

        #[test]
        fn toggle_details_flips_expanded_state() {
            let mut state = state_with_error();
            let now = Instant::now();
            assert!(!state.connection_error.details_expanded);

            reduce(&mut state, Action::ToggleConnectionErrorDetails, now);
            assert!(state.connection_error.details_expanded);

            reduce(&mut state, Action::ToggleConnectionErrorDetails, now);
            assert!(!state.connection_error.details_expanded);
        }

        #[test]
        fn copy_returns_clipboard_effect() {
            let mut state = state_with_error();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::CopyConnectionError, now);

            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::CopyToClipboard { .. }));
        }

        #[test]
        fn copied_marks_feedback_visible() {
            let mut state = state_with_error();
            let now = Instant::now();

            reduce(&mut state, Action::ConnectionErrorCopied, now);

            assert!(state.connection_error.is_copied_visible_at(now));
        }

        #[test]
        fn retry_keeps_modal_open_and_fetches_metadata() {
            let mut state = state_with_error();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.ui.input_mode = InputMode::ConnectionError;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::RetryConnection, now);

            // Error info is preserved during retry (modal stays open)
            assert!(state.connection_error.error_info.is_some());
            assert!(state.connection_error.is_retrying);
            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
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

    mod connection_setup_validation {
        use super::*;
        use rstest::rstest;

        fn setup_state() -> ConnectionSetupState {
            ConnectionSetupState::default()
        }

        #[rstest]
        #[case(ConnectionField::Host, "", true)]
        #[case(ConnectionField::Host, "  ", true)]
        #[case(ConnectionField::Host, "localhost", false)]
        #[case(ConnectionField::Database, "", true)]
        #[case(ConnectionField::Database, "mydb", false)]
        #[case(ConnectionField::User, "", true)]
        #[case(ConnectionField::User, "postgres", false)]
        fn required_field_validation(
            #[case] field: ConnectionField,
            #[case] value: &str,
            #[case] has_error: bool,
        ) {
            let mut state = setup_state();
            match field {
                ConnectionField::Host => state.host = value.to_string(),
                ConnectionField::Database => state.database = value.to_string(),
                ConnectionField::User => state.user = value.to_string(),
                _ => {}
            }

            validate_field(&mut state, field);

            assert_eq!(state.validation_errors.contains_key(&field), has_error);
        }

        #[rstest]
        #[case("", true)]
        #[case("abc", true)]
        #[case("0", true)]
        #[case("1", false)]
        #[case("5432", false)]
        #[case("65535", false)]
        #[case("65536", true)]
        #[case("99999", true)]
        fn port_validation(#[case] value: &str, #[case] has_error: bool) {
            let mut state = setup_state();
            state.port = value.to_string();

            validate_field(&mut state, ConnectionField::Port);

            assert_eq!(
                state.validation_errors.contains_key(&ConnectionField::Port),
                has_error
            );
        }

        #[rstest]
        #[case(ConnectionField::Password)]
        #[case(ConnectionField::SslMode)]
        fn optional_fields_never_error(#[case] field: ConnectionField) {
            let mut state = setup_state();
            state.password = String::new();

            validate_field(&mut state, field);

            assert!(!state.validation_errors.contains_key(&field));
        }

        #[test]
        fn validate_all_checks_all_required_fields() {
            let mut state = setup_state();
            state.host = String::new();
            state.port = "invalid".to_string();
            state.database = String::new();
            state.user = String::new();

            validate_all(&mut state);

            assert!(state.validation_errors.contains_key(&ConnectionField::Host));
            assert!(state.validation_errors.contains_key(&ConnectionField::Port));
            assert!(
                state
                    .validation_errors
                    .contains_key(&ConnectionField::Database)
            );
            assert!(state.validation_errors.contains_key(&ConnectionField::User));
            assert!(
                !state
                    .validation_errors
                    .contains_key(&ConnectionField::Password)
            );
            assert!(
                !state
                    .validation_errors
                    .contains_key(&ConnectionField::SslMode)
            );
        }
    }

    mod connection_setup_transitions {
        use super::*;

        #[test]
        fn save_completed_sets_dsn_and_returns_fetch_effect() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            state.connection_setup.is_first_run = true;
            state.connection_setup.host = "db.example.com".to_string();
            state.connection_setup.port = "5432".to_string();
            state.connection_setup.database = "mydb".to_string();
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ConnectionSaveCompleted {
                    dsn: "postgres://db.example.com/mydb".to_string(),
                },
                now,
            );

            assert!(!state.connection_setup.is_first_run);
            assert_eq!(
                state.runtime.dsn,
                Some("postgres://db.example.com/mydb".to_string())
            );
            assert_eq!(
                state.runtime.active_connection_name,
                Some("db.example.com:5432/mydb".to_string())
            );
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
        }

        #[test]
        fn save_failed_sets_error_message() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ConnectionSaveFailed("Write error".to_string()),
                now,
            );

            assert!(state.messages.last_error.is_some());
            assert!(effects.is_empty());
        }

        #[test]
        fn cancel_on_first_run_opens_confirm_dialog() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            state.connection_setup.is_first_run = true;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ConnectionSetupCancel, now);

            assert_eq!(state.ui.input_mode, InputMode::ConfirmDialog);
            assert!(matches!(state.confirm_dialog.on_confirm, Action::Quit));
            assert!(matches!(
                state.confirm_dialog.on_cancel,
                Action::OpenConnectionSetup
            ));
            assert!(effects.is_empty());
        }

        #[test]
        fn cancel_after_save_returns_to_normal_and_dispatches_try_connect() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConnectionSetup;
            state.connection_setup.is_first_run = false;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::ConnectionSetupCancel, now);

            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::DispatchActions(_)));
        }
    }

    mod confirm_dialog_transitions {
        use super::*;

        #[test]
        fn confirm_executes_on_confirm_action() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.on_confirm = Action::Quit;
            state.confirm_dialog.on_cancel = Action::OpenConnectionSetup;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ConfirmDialogConfirm, now);

            assert!(state.should_quit);
            assert!(matches!(state.confirm_dialog.on_confirm, Action::None));
            assert!(matches!(state.confirm_dialog.on_cancel, Action::None));
        }

        #[test]
        fn cancel_executes_on_cancel_action() {
            let mut state = create_test_state();
            state.ui.input_mode = InputMode::ConfirmDialog;
            state.confirm_dialog.on_confirm = Action::Quit;
            state.confirm_dialog.on_cancel = Action::OpenConnectionSetup;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ConfirmDialogCancel, now);

            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
            assert!(matches!(state.confirm_dialog.on_confirm, Action::None));
            assert!(matches!(state.confirm_dialog.on_cancel, Action::None));
        }
    }

    mod connection_state_tests {
        use super::*;
        use crate::domain::DatabaseMetadata;

        #[test]
        fn try_connect_with_dsn_starts_connecting() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_connecting());
            assert!(matches!(state.cache.state, MetadataState::Loading));
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
        }

        #[test]
        fn try_connect_without_dsn_does_nothing() {
            let mut state = create_test_state();
            state.runtime.dsn = None;
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_not_connected());
            assert!(effects.is_empty());
        }

        #[test]
        fn try_connect_when_already_connecting_is_noop() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::Connecting;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_connecting());
            assert!(effects.is_empty());
        }

        #[test]
        fn try_connect_when_already_connected_is_noop() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::Connected;
            state.ui.input_mode = InputMode::Normal;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_connected());
            assert!(effects.is_empty());
        }

        #[test]
        fn try_connect_when_not_in_normal_mode_is_noop() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.ui.input_mode = InputMode::ConnectionSetup;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::TryConnect, now);

            assert!(state.runtime.connection_state.is_not_connected());
            assert!(effects.is_empty());
        }

        #[test]
        fn metadata_loaded_sets_connected() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Connecting;
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: Instant::now(),
            };
            let now = Instant::now();

            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            assert!(state.runtime.connection_state.is_connected());
            assert!(matches!(state.cache.state, MetadataState::Loaded));
        }

        #[test]
        fn metadata_failed_sets_failed() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Connecting;
            let now = Instant::now();

            let _ = reduce(
                &mut state,
                Action::MetadataFailed("connection refused".to_string()),
                now,
            );

            assert!(state.runtime.connection_state.is_failed());
            assert!(matches!(state.cache.state, MetadataState::Error(_)));
        }

        #[test]
        fn metadata_failed_preserves_connected_state() {
            // When already connected, metadata failure should preserve connection state
            // (metadata-only failure, e.g., permission denied on schema)
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Connected;
            state.cache.state = MetadataState::Loaded;
            let now = Instant::now();

            let _ = reduce(
                &mut state,
                Action::MetadataFailed("permission denied".to_string()),
                now,
            );

            // Connection state should remain Connected
            assert!(state.runtime.connection_state.is_connected());
            // But metadata state should be Error
            assert!(matches!(state.cache.state, MetadataState::Error(_)));
        }

        #[test]
        fn reenter_connection_setup_resets_all_states() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::Failed;
            state.cache.state = MetadataState::Error("error".to_string());
            state.ui.input_mode = InputMode::ConnectionError;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ReenterConnectionSetup, now);

            assert!(state.runtime.connection_state.is_not_connected());
            assert!(matches!(state.cache.state, MetadataState::NotLoaded));
            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
        }

        #[test]
        fn reenter_connection_setup_preserves_form_values() {
            let mut state = create_test_state();
            state.connection_setup.host = "custom-host".to_string();
            state.connection_setup.port = "5433".to_string();
            state.connection_setup.database = "mydb".to_string();
            state.connection_setup.user = "admin".to_string();
            state.connection_setup.password = "secret".to_string();
            state.runtime.connection_state = ConnectionState::Failed;
            let now = Instant::now();

            let _ = reduce(&mut state, Action::ReenterConnectionSetup, now);

            assert_eq!(state.connection_setup.host, "custom-host");
            assert_eq!(state.connection_setup.port, "5433");
            assert_eq!(state.connection_setup.database, "mydb");
            assert_eq!(state.connection_setup.user, "admin");
            assert_eq!(state.connection_setup.password, "secret");
        }

        #[test]
        fn connection_save_completed_sets_connecting_and_loading() {
            let mut state = create_test_state();
            state.runtime.connection_state = ConnectionState::NotConnected;
            state.cache.state = MetadataState::NotLoaded;
            let now = Instant::now();

            let effects = reduce(
                &mut state,
                Action::ConnectionSaveCompleted {
                    dsn: "postgres://localhost/test".to_string(),
                },
                now,
            );

            assert!(state.runtime.connection_state.is_connecting());
            assert!(matches!(state.cache.state, MetadataState::Loading));
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
        }

        #[test]
        fn retry_connection_sets_connecting() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::Failed;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::RetryConnection, now);

            assert!(state.runtime.connection_state.is_connecting());
            assert!(matches!(state.cache.state, MetadataState::Loading));
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], Effect::FetchMetadata { .. }));
        }

        #[test]
        fn retry_connection_without_dsn_redirects_to_setup() {
            let mut state = create_test_state();
            state.runtime.dsn = None;
            state.runtime.connection_state = ConnectionState::Failed;
            state.cache.state = MetadataState::Error("error".to_string());
            state.ui.input_mode = InputMode::ConnectionError;
            let now = Instant::now();

            let effects = reduce(&mut state, Action::RetryConnection, now);

            assert!(state.runtime.connection_state.is_not_connected());
            assert!(matches!(state.cache.state, MetadataState::NotLoaded));
            assert_eq!(state.ui.input_mode, InputMode::ConnectionSetup);
            assert!(effects.is_empty());
        }

        #[test]
        fn retry_then_success_closes_modal_and_shows_footer_message() {
            let mut state = create_test_state();
            state.runtime.dsn = Some("postgres://localhost/test".to_string());
            state.runtime.connection_state = ConnectionState::Failed;
            state.ui.input_mode = InputMode::ConnectionError;
            state.connection_error.error_info = Some(
                crate::app::connection_error::ConnectionErrorInfo::new("test error"),
            );
            let now = Instant::now();

            // Step 1: Retry connection (modal stays open, shows "Retrying...")
            let _ = reduce(&mut state, Action::RetryConnection, now);
            assert!(state.runtime.is_reconnecting);
            assert!(state.connection_error.is_retrying);
            assert_eq!(state.ui.input_mode, InputMode::ConnectionError);

            // Step 2: Metadata loaded (modal closes, footer shows success)
            let metadata = DatabaseMetadata {
                database_name: "test".to_string(),
                schemas: vec![],
                tables: vec![],
                fetched_at: now,
            };
            let _ = reduce(&mut state, Action::MetadataLoaded(Box::new(metadata)), now);

            // Modal is closed, footer shows success message
            assert!(!state.runtime.is_reconnecting);
            assert!(state.connection_error.error_info.is_none());
            assert_eq!(state.ui.input_mode, InputMode::Normal);
            assert_eq!(
                state.messages.last_success,
                Some("Reconnected!".to_string())
            );
        }
    }
}
