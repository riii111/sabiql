mod app;
mod domain;
mod error;
mod infra;
mod ui;

use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use app::action::Action;
use app::command::{command_to_action, parse_command};
use app::completion::CompletionEngine;
use app::er_state::ErStatus;
use app::er_task::{spawn_er_diagram_task, write_er_failure_log_blocking};
use app::input_mode::InputMode;
use app::inspector_tab::InspectorTab;
use app::palette::{palette_action_for_index, palette_command_count};
use app::ports::{MetadataProvider, QueryExecutor};
use app::query_execution::QueryStatus;
use app::state::AppState;
use domain::ErTableInfo;
use domain::MetadataState;
use infra::adapters::PostgresAdapter;
use infra::cache::TtlCache;
use infra::config::{
    cache::get_cache_dir,
    dbx_toml::DbxConfig,
    pgclirc::generate_pgclirc,
    project_root::{find_project_root, get_project_name},
};
use infra::export::DotExporter;
use std::cell::RefCell;
use ui::components::layout::MainLayout;
use ui::components::viewport_columns::{
    calculate_next_column_offset, calculate_prev_column_offset,
};
use ui::event::handler::handle_event;
use ui::tui::TuiRunner;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "default")]
    profile: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    error::install_hooks()?;

    let args = Args::parse();
    let project_root = find_project_root()?;
    let project_name = get_project_name(&project_root);

    let config_path = project_root.join(".dbx.toml");
    let config = if config_path.exists() {
        Some(DbxConfig::load(&config_path)?)
    } else {
        None
    };

    let dsn = config.as_ref().and_then(|c| c.resolve_dsn(&args.profile));
    let _cache_dir = get_cache_dir(&project_name)?;

    // Bounded to prevent unbounded memory growth
    let (action_tx, mut action_rx) = mpsc::channel::<Action>(256);

    let adapter = Arc::new(PostgresAdapter::new());
    let metadata_provider: Arc<dyn MetadataProvider> = Arc::clone(&adapter) as _;
    let query_executor: Arc<dyn QueryExecutor> = Arc::clone(&adapter) as _;
    let metadata_cache = TtlCache::new(300);
    let completion_engine = RefCell::new(CompletionEngine::new());

    let mut state = AppState::new(project_name, args.profile);
    state.runtime.database_name = dsn.as_ref().and_then(|d| extract_database_name(d));
    state.runtime.dsn = dsn.clone();
    state.action_tx = Some(action_tx.clone());

    let mut tui = TuiRunner::new()?.tick_rate(4.0).frame_rate(30.0);
    tui.enter()?;

    let initial_size = tui.terminal().size()?;
    state.ui.terminal_height = initial_size.height;

    if state.runtime.dsn.is_some() {
        let _ = action_tx.send(Action::LoadMetadata).await;
    }

    let cache_cleanup_interval = Duration::from_secs(150);
    let mut last_cache_cleanup = Instant::now();

    loop {
        tokio::select! {
            Some(event) = tui.next_event() => {
                let action = handle_event(event, &state);
                if !action.is_none() {
                    let _ = action_tx.send(action).await;
                }
            }
            Some(action) = action_rx.recv() => {
                handle_action(
                    action,
                    &mut state,
                    &mut tui,
                    &action_tx,
                    &metadata_provider,
                    &query_executor,
                    &metadata_cache,
                    &completion_engine,
                ).await?;
            }
        }

        match state.sql_modal.completion_debounce {
            Some(debounce_until) if Instant::now() >= debounce_until => {
                state.sql_modal.completion_debounce = None;
                let _ = action_tx.send(Action::CompletionTrigger).await;
            }
            _ => (),
        }

        if last_cache_cleanup.elapsed() >= cache_cleanup_interval {
            metadata_cache.cleanup_expired().await;
            last_cache_cleanup = Instant::now();
        }

        if state.should_quit {
            break;
        }
    }

    tui.exit()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_action(
    action: Action,
    state: &mut AppState,
    tui: &mut TuiRunner,
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    query_executor: &Arc<dyn QueryExecutor>,
    metadata_cache: &TtlCache<String, domain::DatabaseMetadata>,
    completion_engine: &RefCell<CompletionEngine>,
) -> Result<()> {
    match action {
        Action::Quit => state.should_quit = true,
        Action::Render => {
            state.clear_expired_messages();
            tui.terminal()
                .draw(|frame| MainLayout::render(frame, state))?;
        }
        Action::Resize(_w, h) => {
            // Ratatui auto-tracks size; explicit resize() restricts viewport
            state.ui.terminal_height = h;
        }
        Action::SetFocusedPane(pane) => state.ui.focused_pane = pane,
        Action::ToggleFocus => {
            state.toggle_focus();
        }

        Action::InspectorNextTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.next();
        }
        Action::InspectorPrevTab => {
            state.ui.inspector_tab = state.ui.inspector_tab.prev();
        }

        Action::OpenTablePicker => {
            state.ui.input_mode = InputMode::TablePicker;
            state.ui.filter_input.clear();
            state.ui.picker_selected = 0;
        }
        Action::CloseTablePicker => {
            state.ui.input_mode = InputMode::Normal;
        }
        Action::OpenCommandPalette => {
            state.ui.input_mode = InputMode::CommandPalette;
            state.ui.picker_selected = 0;
        }
        Action::CloseCommandPalette => {
            state.ui.input_mode = InputMode::Normal;
        }
        Action::OpenHelp => {
            state.ui.input_mode = if state.ui.input_mode == InputMode::Help {
                InputMode::Normal
            } else {
                InputMode::Help
            };
        }
        Action::CloseHelp => {
            state.ui.input_mode = InputMode::Normal;
        }

        Action::OpenSqlModal => {
            state.ui.input_mode = InputMode::SqlModal;
            state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion.candidates.clear();
            state.sql_modal.completion.selected_index = 0;
            state.sql_modal.completion_debounce = None;
            if !state.sql_modal.prefetch_started && state.cache.metadata.is_some() {
                let _ = action_tx.send(Action::StartPrefetchAll).await;
            }
        }
        Action::CloseSqlModal => {
            state.ui.input_mode = InputMode::Normal;
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
            // Keep prefetch running for ER diagram usage
        }
        Action::SqlModalInput(c) => {
            state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, c);
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(Instant::now() + Duration::from_millis(100));
        }
        Action::SqlModalBackspace => {
            state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
            if state.sql_modal.cursor > 0 {
                state.sql_modal.cursor -= 1;
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(Instant::now() + Duration::from_millis(100));
        }
        Action::SqlModalDelete => {
            state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
            let total_chars = char_count(&state.sql_modal.content);
            if state.sql_modal.cursor < total_chars {
                let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                state.sql_modal.content.remove(byte_idx);
            }
            state.sql_modal.completion_debounce = Some(Instant::now() + Duration::from_millis(100));
        }
        Action::SqlModalNewLine => {
            state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert(byte_idx, '\n');
            state.sql_modal.cursor += 1;
            state.sql_modal.completion_debounce = Some(Instant::now() + Duration::from_millis(100));
        }
        Action::SqlModalTab => {
            state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
            let byte_idx = char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
            state.sql_modal.content.insert_str(byte_idx, "    ");
            state.sql_modal.cursor += 4;
            state.sql_modal.completion_debounce = Some(Instant::now() + Duration::from_millis(100));
        }
        Action::SqlModalMoveCursor(movement) => {
            use app::action::CursorMove;
            let content = &state.sql_modal.content;
            let cursor = state.sql_modal.cursor;
            let total_chars = char_count(content);

            let lines: Vec<(usize, usize)> = {
                let mut result = Vec::new();
                let mut start = 0;
                for line in content.split('\n') {
                    let len = line.chars().count();
                    result.push((start, len));
                    start += len + 1; // +1 for '\n'
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
        }
        Action::SqlModalSubmit => {
            let query = state.sql_modal.content.trim().to_string();
            if !query.is_empty() {
                state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Running;
                state.sql_modal.completion.visible = false;
                let _ = action_tx.send(Action::ExecuteAdhoc(query)).await;
            }
        }
        Action::SqlModalClear => {
            state.sql_modal.clear_content();
        }

        Action::CompletionTrigger => {
            let cursor = state.sql_modal.cursor;

            // Scoped borrow to release before async operations
            let missing = {
                let engine = completion_engine.borrow();
                engine.missing_tables(&state.sql_modal.content, state.cache.metadata.as_ref())
            };

            for qualified_name in missing {
                // Only prefetch if schema is known (resolved from metadata)
                if let Some((schema, table)) = qualified_name.split_once('.') {
                    let _ = action_tx
                        .send(Action::PrefetchTableDetail {
                            schema: schema.to_string(),
                            table: table.to_string(),
                        })
                        .await;
                }
            }

            let engine = completion_engine.borrow();
            let token_len = engine.current_token_len(&state.sql_modal.content, cursor);
            let recent_cols = state.sql_modal.completion.recent_columns_vec();
            let candidates = engine.get_candidates(
                &state.sql_modal.content,
                cursor,
                state.cache.metadata.as_ref(),
                state.cache.table_detail.as_ref(),
                &recent_cols,
            );
            state.sql_modal.completion.candidates = candidates;
            state.sql_modal.completion.selected_index = 0;
            state.sql_modal.completion.visible = !state.sql_modal.completion.candidates.is_empty()
                && !state.sql_modal.content.trim().is_empty();
            state.sql_modal.completion.trigger_position = cursor.saturating_sub(token_len);
        }
        Action::CompletionAccept => {
            if state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty()
            {
                if let Some(candidate) = state
                    .sql_modal
                    .completion
                    .candidates
                    .get(state.sql_modal.completion.selected_index)
                {
                    let insert_text = candidate.text.clone();
                    let trigger_pos = state.sql_modal.completion.trigger_position;

                    let start_byte = char_to_byte_index(&state.sql_modal.content, trigger_pos);
                    let end_byte =
                        char_to_byte_index(&state.sql_modal.content, state.sql_modal.cursor);
                    state.sql_modal.content.drain(start_byte..end_byte);

                    state.sql_modal.content.insert_str(start_byte, &insert_text);
                    state.sql_modal.cursor = trigger_pos + insert_text.chars().count();
                }
                state.sql_modal.completion.visible = false;
                state.sql_modal.completion.candidates.clear();
                state.sql_modal.completion_debounce = None;
            }
        }
        Action::CompletionDismiss => {
            state.sql_modal.completion.visible = false;
            state.sql_modal.completion_debounce = None;
        }
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
        }

        Action::EnterCommandLine => {
            state.ui.input_mode = InputMode::CommandLine;
            state.command_line_input.clear();
        }
        Action::ExitCommandLine => {
            state.ui.input_mode = InputMode::Normal;
        }
        Action::CommandLineInput(c) => {
            state.command_line_input.push(c);
        }
        Action::CommandLineBackspace => {
            state.command_line_input.pop();
        }
        Action::CommandLineSubmit => {
            let cmd = parse_command(&state.command_line_input);
            let follow_up = command_to_action(cmd);
            state.ui.input_mode = InputMode::Normal;
            state.command_line_input.clear();
            match follow_up {
                Action::Quit => state.should_quit = true,
                Action::OpenHelp => state.ui.input_mode = InputMode::Help,
                Action::OpenSqlModal => {
                    state.ui.input_mode = InputMode::SqlModal;
                    state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
                }
                Action::OpenConsole => {
                    let _ = action_tx.send(Action::OpenConsole).await;
                }
                Action::ErOpenDiagram => {
                    let _ = action_tx.send(follow_up).await;
                }
                _ => {}
            }
        }

        Action::FilterInput(c) => {
            state.ui.filter_input.push(c);
            state.ui.picker_selected = 0;
        }
        Action::FilterBackspace => {
            state.ui.filter_input.pop();
            state.ui.picker_selected = 0;
        }

        Action::SelectNext => match state.ui.input_mode {
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
                if state.ui.focused_pane == app::focused_pane::FocusedPane::Explorer {
                    let max = state.tables().len().saturating_sub(1);
                    if state.ui.explorer_selected < max {
                        state.ui.explorer_selected += 1;
                    }
                }
            }
            _ => {}
        },
        Action::SelectPrevious => match state.ui.input_mode {
            InputMode::TablePicker | InputMode::CommandPalette => {
                state.ui.picker_selected = state.ui.picker_selected.saturating_sub(1);
            }
            InputMode::Normal => {
                if state.ui.focused_pane == app::focused_pane::FocusedPane::Explorer {
                    state.ui.explorer_selected = state.ui.explorer_selected.saturating_sub(1);
                }
            }
            _ => {}
        },
        Action::SelectFirst => match state.ui.input_mode {
            InputMode::TablePicker | InputMode::CommandPalette => {
                state.ui.picker_selected = 0;
            }
            InputMode::Normal => {
                if state.ui.focused_pane == app::focused_pane::FocusedPane::Explorer {
                    state.ui.explorer_selected = 0;
                }
            }
            _ => {}
        },
        Action::SelectLast => match state.ui.input_mode {
            InputMode::TablePicker => {
                let max = state.filtered_tables().len().saturating_sub(1);
                state.ui.picker_selected = max;
            }
            InputMode::CommandPalette => {
                state.ui.picker_selected = palette_command_count() - 1;
            }
            InputMode::Normal => {
                if state.ui.focused_pane == app::focused_pane::FocusedPane::Explorer {
                    state.ui.explorer_selected = state.tables().len().saturating_sub(1);
                }
            }
            _ => {}
        },

        Action::ConfirmSelection => {
            if state.ui.input_mode == InputMode::TablePicker {
                let filtered = state.filtered_tables();
                if let Some(table) = filtered.get(state.ui.picker_selected) {
                    let schema = table.schema.clone();
                    let table_name = table.name.clone();
                    state.cache.current_table = Some(table.qualified_name());
                    state.ui.input_mode = InputMode::Normal;

                    // Increment generation to invalidate any in-flight requests
                    state.cache.selection_generation += 1;
                    let current_gen = state.cache.selection_generation;

                    // Trigger table detail loading and preview (sequential to avoid rate limits)
                    // TODO: If performance becomes an issue, consider parallel execution
                    // with a semaphore to limit concurrency (e.g., tokio::sync::Semaphore)
                    let _ = action_tx
                        .send(Action::LoadTableDetail {
                            schema: schema.clone(),
                            table: table_name.clone(),
                            generation: current_gen,
                        })
                        .await;
                    let _ = action_tx
                        .send(Action::ExecutePreview {
                            schema,
                            table: table_name,
                            generation: current_gen,
                        })
                        .await;
                }
            } else if state.ui.input_mode == InputMode::Normal
                && state.ui.focused_pane == app::focused_pane::FocusedPane::Explorer
            {
                let tables = state.tables();
                if let Some(table) = tables.get(state.ui.explorer_selected) {
                    let schema = table.schema.clone();
                    let table_name = table.name.clone();
                    state.cache.current_table = Some(table.qualified_name());

                    state.cache.selection_generation += 1;
                    let current_gen = state.cache.selection_generation;

                    // TODO: If performance becomes an issue, consider parallel execution
                    // with a semaphore to limit concurrency (e.g., tokio::sync::Semaphore)
                    let _ = action_tx
                        .send(Action::LoadTableDetail {
                            schema: schema.clone(),
                            table: table_name.clone(),
                            generation: current_gen,
                        })
                        .await;
                    let _ = action_tx
                        .send(Action::ExecutePreview {
                            schema,
                            table: table_name,
                            generation: current_gen,
                        })
                        .await;
                }
            } else if state.ui.input_mode == InputMode::CommandPalette {
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
                        state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Editing;
                    }
                    Action::ReloadMetadata => {
                        if let Some(dsn) = &state.runtime.dsn {
                            metadata_cache.invalidate(dsn).await;
                            let _ = action_tx.send(Action::LoadMetadata).await;
                        }
                    }
                    Action::OpenConsole => {
                        let _ = action_tx.send(Action::OpenConsole).await;
                    }
                    _ => {}
                }
            }
        }

        Action::Escape => {
            state.ui.input_mode = InputMode::Normal;
        }

        Action::LoadMetadata => {
            if let Some(dsn) = &state.runtime.dsn {
                if let Some(cached) = metadata_cache.get(dsn).await {
                    state.cache.metadata = Some(cached);
                    state.cache.state = MetadataState::Loaded;
                } else {
                    state.cache.state = MetadataState::Loading;

                    let dsn = dsn.clone();
                    let provider = Arc::clone(metadata_provider);
                    let cache = metadata_cache.clone();
                    let tx = action_tx.clone();

                    tokio::spawn(async move {
                        match provider.fetch_metadata(&dsn).await {
                            Ok(metadata) => {
                                cache.set(dsn, metadata.clone()).await;
                                // Use send().await for critical events to ensure delivery
                                let _ = tx.send(Action::MetadataLoaded(Box::new(metadata))).await;
                            }
                            Err(e) => {
                                let _ = tx.send(Action::MetadataFailed(e.to_string())).await;
                            }
                        }
                    });
                }
            }
        }

        Action::ReloadMetadata => {
            if let Some(dsn) = &state.runtime.dsn {
                metadata_cache.invalidate(dsn).await;

                state.sql_modal.reset_prefetch();
                completion_engine.borrow_mut().clear_table_cache();

                // Reset ER preparation state and clear stale messages
                state.er_preparation.reset();
                state.messages.clear();

                let _ = action_tx.send(Action::LoadMetadata).await;
            }
        }

        Action::MetadataLoaded(metadata) => {
            state.cache.metadata = Some(*metadata);
            state.cache.state = MetadataState::Loaded;

            // Start prefetching table details for completion and ER diagrams
            if !state.sql_modal.prefetch_started {
                let _ = action_tx.send(Action::StartPrefetchAll).await;
            }
        }

        Action::MetadataFailed(error) => {
            state.cache.state = MetadataState::Error(error);
        }

        Action::LoadTableDetail {
            schema,
            table,
            generation,
        } => {
            if let Some(dsn) = &state.runtime.dsn {
                let dsn = dsn.clone();
                let provider = Arc::clone(metadata_provider);
                let tx = action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_table_detail(&dsn, &schema, &table).await {
                        Ok(detail) => {
                            let _ = tx
                                .send(Action::TableDetailLoaded(Box::new(detail), generation))
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::TableDetailFailed(e.to_string(), generation))
                                .await;
                        }
                    }
                });
            }
        }

        Action::TableDetailLoaded(detail, generation) => {
            // Ignore stale results from previous table selections
            if generation == state.cache.selection_generation {
                // Cache for completion to avoid redundant prefetch for the selected table
                completion_engine
                    .borrow_mut()
                    .cache_table_detail(detail.qualified_name(), (*detail).clone());
                state.cache.table_detail = Some(*detail);
                state.ui.inspector_scroll_offset = 0;
            }
        }

        Action::TableDetailFailed(error, generation) => {
            // Ignore stale errors from previous table selections
            if generation == state.cache.selection_generation {
                state.set_error(error);
            }
        }

        Action::PrefetchTableDetail { schema, table } => {
            const PREFETCH_BACKOFF_SECS: u64 = 30;
            let qualified_name = format!("{}.{}", schema, table);

            let recently_failed = state
                .sql_modal
                .failed_prefetch_tables
                .get(&qualified_name)
                .map(|(t, _)| t.elapsed().as_secs() < PREFETCH_BACKOFF_SECS)
                .unwrap_or(false);

            if state.sql_modal.prefetching_tables.contains(&qualified_name)
                || completion_engine.borrow().has_cached_table(&qualified_name)
                || recently_failed
            {
                state.er_preparation.pending_tables.remove(&qualified_name);

                // Check if ER preparation completed after this skip
                if state.er_preparation.status == ErStatus::Waiting
                    && state.er_preparation.is_complete()
                {
                    state.er_preparation.status = ErStatus::Idle;
                    if !state.er_preparation.has_failures() {
                        state.set_success("ER ready. Press 'e' to open.".to_string());
                    } else {
                        let failed_count = state.er_preparation.failed_tables.len();
                        let log_written =
                            if let Ok(cache_dir) = get_cache_dir(&state.runtime.project_name) {
                                let failed_data: Vec<(String, String)> = state
                                    .er_preparation
                                    .failed_tables
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.clone()))
                                    .collect();
                                tokio::task::spawn_blocking(move || {
                                    write_er_failure_log_blocking(failed_data, cache_dir).is_ok()
                                })
                                .await
                                .unwrap_or(false)
                            } else {
                                false
                            };
                        let msg = if log_written {
                            format!(
                                "ER failed: {} table(s) failed. See log for details. 'e' to retry.",
                                failed_count
                            )
                        } else {
                            format!("ER failed: {} table(s) failed. 'e' to retry.", failed_count)
                        };
                        state.set_error(msg);
                    }
                }
            } else if let Some(dsn) = &state.runtime.dsn {
                state
                    .sql_modal
                    .prefetching_tables
                    .insert(qualified_name.clone());
                state.er_preparation.pending_tables.remove(&qualified_name);
                state.er_preparation.fetching_tables.insert(qualified_name);
                let dsn = dsn.clone();
                let provider = Arc::clone(metadata_provider);
                let tx = action_tx.clone();

                tokio::spawn(async move {
                    match provider.fetch_table_detail(&dsn, &schema, &table).await {
                        Ok(detail) => {
                            let _ = tx
                                .send(Action::TableDetailCached {
                                    schema,
                                    table,
                                    detail: Box::new(detail),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::TableDetailCacheFailed {
                                    schema,
                                    table,
                                    error: e.to_string(),
                                })
                                .await;
                        }
                    }
                });
            }
        }

        Action::TableDetailCached {
            schema,
            table,
            detail,
        } => {
            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .remove(&qualified_name);
            state.er_preparation.on_table_cached(&qualified_name);
            completion_engine
                .borrow_mut()
                .cache_table_detail(qualified_name, *detail);

            if state.ui.input_mode == InputMode::SqlModal && state.sql_modal.prefetch_queue.is_empty()
            {
                state.sql_modal.completion_debounce = None;
                let _ = action_tx.send(Action::CompletionTrigger).await;
            }
            if !state.sql_modal.prefetch_queue.is_empty() {
                let _ = action_tx.send(Action::ProcessPrefetchQueue).await;
            }

            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                if !state.er_preparation.has_failures() {
                    state.set_success("ER ready. Press 'e' to open.".to_string());
                } else {
                    let failed_count = state.er_preparation.failed_tables.len();
                    let log_written =
                        if let Ok(cache_dir) = get_cache_dir(&state.runtime.project_name) {
                            let failed_data: Vec<(String, String)> = state
                                .er_preparation
                                .failed_tables
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            tokio::task::spawn_blocking(move || {
                                write_er_failure_log_blocking(failed_data, cache_dir).is_ok()
                            })
                            .await
                            .unwrap_or(false)
                        } else {
                            false
                        };
                    let msg = if log_written {
                        format!(
                            "ER failed: {} table(s) failed. See log for details. 'e' to retry.",
                            failed_count
                        )
                    } else {
                        format!("ER failed: {} table(s) failed. 'e' to retry.", failed_count)
                    };
                    state.set_error(msg);
                }
            }
        }

        Action::TableDetailCacheFailed {
            schema,
            table,
            error,
        } => {
            let qualified_name = format!("{}.{}", schema, table);
            state.sql_modal.prefetching_tables.remove(&qualified_name);
            state
                .sql_modal
                .failed_prefetch_tables
                .insert(qualified_name.clone(), (Instant::now(), error.clone()));
            state.er_preparation.on_table_failed(&qualified_name, error);
            if !state.sql_modal.prefetch_queue.is_empty() {
                let _ = action_tx.send(Action::ProcessPrefetchQueue).await;
            }

            // Notify user when prefetch completes while in Waiting state
            if state.er_preparation.status == ErStatus::Waiting
                && state.er_preparation.is_complete()
            {
                state.er_preparation.status = ErStatus::Idle;
                let failed_count = state.er_preparation.failed_tables.len();
                let log_written = if let Ok(cache_dir) = get_cache_dir(&state.runtime.project_name)
                {
                    let failed_data: Vec<(String, String)> = state
                        .er_preparation
                        .failed_tables
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    tokio::task::spawn_blocking(move || {
                        write_er_failure_log_blocking(failed_data, cache_dir).is_ok()
                    })
                    .await
                    .unwrap_or(false)
                } else {
                    false
                };
                let msg = if log_written {
                    format!(
                        "ER failed: {} table(s) failed. See log for details. 'e' to retry.",
                        failed_count
                    )
                } else {
                    format!("ER failed: {} table(s) failed. 'e' to retry.", failed_count)
                };
                state.set_error(msg);
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
                {
                    let engine = completion_engine.borrow();
                    for table_summary in &metadata.tables {
                        let qualified_name = table_summary.qualified_name();
                        if !engine.has_cached_table(&qualified_name) {
                            state
                                .sql_modal
                                .prefetch_queue
                                .push_back(qualified_name.clone());
                            state.er_preparation.pending_tables.insert(qualified_name);
                        }
                    }
                }
                let _ = action_tx.send(Action::ProcessPrefetchQueue).await;
            }
        }

        Action::ProcessPrefetchQueue => {
            const MAX_CONCURRENT_PREFETCH: usize = 4;
            let current_in_flight = state.sql_modal.prefetching_tables.len();
            let available_slots = MAX_CONCURRENT_PREFETCH.saturating_sub(current_in_flight);

            for _ in 0..available_slots {
                if let Some(qualified_name) = state.sql_modal.prefetch_queue.pop_front() {
                    if let Some((schema, table)) = qualified_name.split_once('.') {
                        let _ = action_tx
                            .send(Action::PrefetchTableDetail {
                                schema: schema.to_string(),
                                table: table.to_string(),
                            })
                            .await;
                    } else {
                        debug_assert!(false, "Invalid qualified_name format: {}", qualified_name);
                    }
                }
            }
        }

        Action::ExecutePreview {
            schema,
            table,
            generation,
        } => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(std::time::Instant::now());
                let dsn = dsn.clone();
                let tx = action_tx.clone();

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

                let executor = query_executor.clone();
                tokio::spawn(async move {
                    match executor.execute_preview(&dsn, &schema, &table, limit).await {
                        Ok(result) => {
                            let _ = tx
                                .send(Action::QueryCompleted(Box::new(result), generation))
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Action::QueryFailed(e.to_string(), generation))
                                .await;
                        }
                    }
                });
            }
        }

        Action::ExecuteAdhoc(query) => {
            if let Some(dsn) = &state.runtime.dsn {
                state.query.status = QueryStatus::Running;
                state.query.start_time = Some(std::time::Instant::now());
                let dsn = dsn.clone();
                let tx = action_tx.clone();

                let executor = query_executor.clone();
                tokio::spawn(async move {
                    match executor.execute_adhoc(&dsn, &query).await {
                        Ok(result) => {
                            // Adhoc queries use generation 0 to always show results
                            let _ = tx.send(Action::QueryCompleted(Box::new(result), 0)).await;
                        }
                        Err(e) => {
                            // Adhoc queries use generation 0 to always show errors
                            let _ = tx.send(Action::QueryFailed(e.to_string(), 0)).await;
                        }
                    }
                });
            }
        }

        Action::QueryCompleted(result, generation) => {
            // For Preview (non-zero generation), check if this is still the current selection
            // For Adhoc (generation 0), always show results
            if generation == 0 || generation == state.cache.selection_generation {
                state.query.status = QueryStatus::Idle;
                state.query.start_time = None;
                state.ui.result_scroll_offset = 0;
                state.ui.result_horizontal_offset = 0;
                state.query.result_highlight_until =
                    Some(Instant::now() + Duration::from_millis(500));
                state.query.history_index = None;

                if result.source == domain::QuerySource::Adhoc {
                    if result.is_error() {
                        state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Error;
                    } else {
                        state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Success;
                    }
                }

                // Save adhoc results to history
                if result.source == domain::QuerySource::Adhoc && !result.is_error() {
                    state.query.result_history.push((*result).clone());
                }

                state.query.current_result = Some(*result);
            }
        }

        Action::QueryFailed(error, generation) => {
            // For Preview (non-zero generation), check if this is still the current selection
            // For Adhoc (generation 0), always show errors
            if generation == 0 || generation == state.cache.selection_generation {
                state.query.status = QueryStatus::Idle;
                state.query.start_time = None;
                state.set_error(error.clone());
                // If we're in SqlModal mode, set error state and show error in result pane
                if state.ui.input_mode == InputMode::SqlModal {
                    state.sql_modal.status = app::sql_modal_context::SqlModalStatus::Error;
                    // Show error in result pane for better visibility
                    let error_result = domain::QueryResult::error(
                        state.sql_modal.content.clone(),
                        error,
                        0,
                        domain::QuerySource::Adhoc,
                    );
                    state.query.current_result = Some(error_result);
                }
            }
        }

        Action::ResultScrollUp => {
            state.ui.result_scroll_offset = state.ui.result_scroll_offset.saturating_sub(1);
        }

        Action::ResultScrollDown => {
            // We need the result to determine max scroll
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
        }

        Action::ResultScrollTop => {
            state.ui.result_scroll_offset = 0;
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
        }

        Action::ResultScrollLeft => {
            state.ui.result_horizontal_offset =
                calculate_prev_column_offset(state.ui.result_horizontal_offset);
        }

        Action::ResultScrollRight => {
            let plan = &state.ui.result_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.result_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.result_horizontal_offset,
                plan.column_count,
            );
        }

        Action::InspectorScrollUp => {
            state.ui.inspector_scroll_offset = state.ui.inspector_scroll_offset.saturating_sub(1);
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
                    InspectorTab::Rls => {
                        // RLS: status line + blank + header + policies (each 1-2 lines)
                        t.rls.as_ref().map_or(1, |rls| {
                            let mut lines = 1; // Status line
                            if !rls.policies.is_empty() {
                                lines += 2; // blank + "Policies:" header
                                for policy in &rls.policies {
                                    lines += 1; // policy line
                                    if policy.qual.is_some() {
                                        lines += 1; // USING line
                                    }
                                }
                            }
                            lines
                        })
                    }
                    InspectorTab::Ddl => ui::components::inspector::Inspector::ddl_line_count(t),
                })
                .unwrap_or(0);
            let max_offset = total_items.saturating_sub(visible);
            if state.ui.inspector_scroll_offset < max_offset {
                state.ui.inspector_scroll_offset += 1;
            }
        }

        Action::InspectorScrollLeft => {
            state.ui.inspector_horizontal_offset =
                calculate_prev_column_offset(state.ui.inspector_horizontal_offset);
        }

        Action::InspectorScrollRight => {
            let plan = &state.ui.inspector_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.inspector_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.inspector_horizontal_offset,
                plan.column_count,
            );
        }

        Action::ExplorerScrollLeft => {
            state.ui.explorer_horizontal_offset = state.ui.explorer_horizontal_offset.saturating_sub(1);
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
        }

        Action::OpenConsole => {
            if let Some(dsn) = &state.runtime.dsn {
                let cache_dir = get_cache_dir(&state.runtime.project_name)?;
                let pgclirc = generate_pgclirc(&cache_dir)?;

                let guard = tui.suspend_guard()?;

                let dsn = dsn.clone();
                let status = tokio::task::spawn_blocking(move || {
                    std::process::Command::new("pgcli")
                        .arg("--pgclirc")
                        .arg(&pgclirc)
                        .arg(&dsn)
                        .status()
                })
                .await;

                guard.resume()?;

                match status {
                    Err(e) => {
                        state.set_error(format!("pgcli task failed: {}", e));
                    }
                    Ok(Err(e)) => {
                        state.set_error(format!("pgcli failed to start: {}", e));
                    }
                    Ok(Ok(exit_status)) if !exit_status.success() => {
                        let code = exit_status
                            .code()
                            .map_or("unknown".to_string(), |c| c.to_string());
                        state.set_error(format!("pgcli exited with code {}", code));
                    }
                    Ok(Ok(_)) => {}
                }

                let _ = action_tx.send(Action::Render).await;
            } else {
                state.set_error("No DSN configured".to_string());
            }
        }

        Action::ErOpenDiagram => {
            // Guard: ignore if already rendering or waiting
            if matches!(
                state.er_preparation.status,
                ErStatus::Rendering | ErStatus::Waiting
            ) {
                return Ok(());
            }

            // Retry failed tables if any
            if state.er_preparation.has_failures() {
                let failed_tables: Vec<String> =
                    state.er_preparation.failed_tables.keys().cloned().collect();
                state.er_preparation.retry_failed();
                state.sql_modal.failed_prefetch_tables.clear();

                for qualified_name in failed_tables {
                    state.sql_modal.prefetch_queue.push_back(qualified_name);
                }

                state.er_preparation.status = ErStatus::Waiting;
                let _ = action_tx.send(Action::ProcessPrefetchQueue).await;
                return Ok(());
            }

            // Check if prefetch is complete
            if !state.er_preparation.is_complete() {
                state.er_preparation.status = ErStatus::Waiting;
                return Ok(());
            }

            // Collect lightweight snapshots for DOT generation
            let tables: Vec<ErTableInfo> = {
                let engine = completion_engine.borrow();
                engine
                    .table_details_iter()
                    .map(|(k, v)| ErTableInfo::from_table(k, v))
                    .collect()
            };

            if tables.is_empty() {
                state.set_error("No table data loaded yet".to_string());
                return Ok(());
            }

            state.er_preparation.status = ErStatus::Rendering;
            let total_tables = state
                .cache
                .metadata
                .as_ref()
                .map(|m| m.tables.len())
                .unwrap_or(0);
            let cache_dir = get_cache_dir(&state.runtime.project_name)?;

            let exporter = Arc::new(DotExporter::new());
            spawn_er_diagram_task(exporter, tables, total_tables, cache_dir, action_tx.clone());
        }

        Action::ErDiagramOpened {
            path,
            table_count,
            total_tables,
        } => {
            state.er_preparation.status = ErStatus::Idle;
            state.set_success(format!(
                "✓ Opened {} ({}/{} tables)",
                path, table_count, total_tables
            ));
        }

        Action::ErDiagramFailed(error) => {
            state.er_preparation.status = ErStatus::Idle;
            state.set_error(error);
        }

        _ => {}
    }

    Ok(())
}

fn extract_database_name(dsn: &str) -> Option<String> {
    let name = PostgresAdapter::extract_database_name(dsn);
    if name == "unknown" { None } else { Some(name) }
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
