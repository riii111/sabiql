mod app;
mod domain;
mod error;
mod infra;
mod ui;

use std::sync::Arc;

use clap::Parser;
use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use app::action::Action;
use app::command::{command_to_action, parse_command};
use app::input_mode::InputMode;
use app::palette::{palette_action_for_index, palette_command_count};
use app::ports::MetadataProvider;
use app::state::AppState;
use domain::MetadataState;
use infra::adapters::PostgresAdapter;
use infra::cache::TtlCache;
use infra::config::{
    cache::get_cache_dir,
    dbx_toml::DbxConfig,
    project_root::{find_project_root, get_project_name},
};
use ui::components::layout::MainLayout;
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

    // Action channel for async communication (bounded to prevent unbounded memory growth)
    let (action_tx, mut action_rx) = mpsc::channel::<Action>(256);

    // Metadata provider and cache
    let metadata_provider: Arc<dyn MetadataProvider> = Arc::new(PostgresAdapter::new());
    let metadata_cache = TtlCache::new(300); // 5 min TTL

    let mut state = AppState::new(project_name, args.profile);
    state.database_name = dsn.as_ref().and_then(|d| extract_database_name(d));
    state.dsn = dsn.clone();
    state.action_tx = Some(action_tx.clone());

    let mut tui = TuiRunner::new()?.tick_rate(4.0).frame_rate(30.0);
    tui.enter()?;

    // Load metadata on startup if DSN is available
    if state.dsn.is_some() {
        let _ = action_tx.send(Action::LoadMetadata).await;
    }

    loop {
        tokio::select! {
            Some(event) = tui.next_event() => {
                let action = handle_event(event, &state);
                if !action.is_none() {
                    // Use send().await for user input to ensure no key events are lost
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
                    &metadata_cache,
                ).await?;
            }
        }

        if state.should_quit {
            break;
        }
    }

    tui.exit()?;
    Ok(())
}

async fn handle_action(
    action: Action,
    state: &mut AppState,
    tui: &mut TuiRunner,
    action_tx: &mpsc::Sender<Action>,
    metadata_provider: &Arc<dyn MetadataProvider>,
    metadata_cache: &TtlCache<String, domain::DatabaseMetadata>,
) -> Result<()> {
    match action {
        Action::Quit => state.should_quit = true,
        Action::Render => {
            tui.terminal()
                .draw(|frame| MainLayout::render(frame, state))?;
        }
        Action::Resize(w, h) => {
            tui.terminal()
                .resize(ratatui::layout::Rect::new(0, 0, w, h))?;
        }
        Action::NextTab => {
            const TAB_COUNT: usize = 2;
            state.active_tab = (state.active_tab + 1) % TAB_COUNT;
        }
        Action::PreviousTab => {
            const TAB_COUNT: usize = 2;
            state.active_tab = (state.active_tab + TAB_COUNT - 1) % TAB_COUNT;
        }
        Action::ToggleFocus => state.focus_mode = !state.focus_mode,

        Action::OpenTablePicker => {
            state.input_mode = InputMode::TablePicker;
            state.filter_input.clear();
            state.picker_selected = 0;
        }
        Action::CloseTablePicker => {
            state.input_mode = InputMode::Normal;
        }
        Action::OpenCommandPalette => {
            state.input_mode = InputMode::CommandPalette;
            state.picker_selected = 0;
        }
        Action::CloseCommandPalette => {
            state.input_mode = InputMode::Normal;
        }
        Action::OpenHelp => {
            state.input_mode = if state.input_mode == InputMode::Help {
                InputMode::Normal
            } else {
                InputMode::Help
            };
        }
        Action::CloseHelp => {
            state.input_mode = InputMode::Normal;
        }

        // Command line actions
        Action::EnterCommandLine => {
            state.input_mode = InputMode::CommandLine;
            state.command_line_input.clear();
        }
        Action::ExitCommandLine => {
            state.input_mode = InputMode::Normal;
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
            state.input_mode = InputMode::Normal;
            state.command_line_input.clear();
            if matches!(follow_up, Action::Quit) {
                state.should_quit = true;
            } else if matches!(follow_up, Action::OpenHelp) {
                state.input_mode = InputMode::Help;
            }
        }

        // Filter actions
        Action::FilterInput(c) => {
            state.filter_input.push(c);
            state.picker_selected = 0;
        }
        Action::FilterBackspace => {
            state.filter_input.pop();
            state.picker_selected = 0;
        }
        Action::FilterClear => {
            state.filter_input.clear();
            state.picker_selected = 0;
        }

        Action::SelectNext => match state.input_mode {
            InputMode::TablePicker => {
                let max = state.filtered_tables().len().saturating_sub(1);
                if state.picker_selected < max {
                    state.picker_selected += 1;
                }
            }
            InputMode::CommandPalette => {
                let max = palette_command_count() - 1;
                if state.picker_selected < max {
                    state.picker_selected += 1;
                }
            }
            InputMode::Normal => {
                let max = state.tables().len().saturating_sub(1);
                if state.explorer_selected < max {
                    state.explorer_selected += 1;
                }
            }
            _ => {}
        },
        Action::SelectPrevious => match state.input_mode {
            InputMode::TablePicker | InputMode::CommandPalette => {
                state.picker_selected = state.picker_selected.saturating_sub(1);
            }
            InputMode::Normal => {
                state.explorer_selected = state.explorer_selected.saturating_sub(1);
            }
            _ => {}
        },
        Action::SelectFirst => match state.input_mode {
            InputMode::TablePicker | InputMode::CommandPalette => {
                state.picker_selected = 0;
            }
            InputMode::Normal => {
                state.explorer_selected = 0;
            }
            _ => {}
        },
        Action::SelectLast => match state.input_mode {
            InputMode::TablePicker => {
                let max = state.filtered_tables().len().saturating_sub(1);
                state.picker_selected = max;
            }
            InputMode::CommandPalette => {
                state.picker_selected = palette_command_count() - 1;
            }
            InputMode::Normal => {
                state.explorer_selected = state.tables().len().saturating_sub(1);
            }
            _ => {}
        },

        Action::ConfirmSelection => {
            if state.input_mode == InputMode::TablePicker {
                let filtered = state.filtered_tables();
                if let Some(table) = filtered.get(state.picker_selected) {
                    state.current_table = Some(table.qualified_name());
                    state.input_mode = InputMode::Normal;
                }
            } else if state.input_mode == InputMode::CommandPalette {
                let cmd_action = palette_action_for_index(state.picker_selected);
                state.input_mode = InputMode::Normal;
                match cmd_action {
                    Action::Quit => state.should_quit = true,
                    Action::OpenHelp => state.input_mode = InputMode::Help,
                    Action::OpenTablePicker => {
                        state.input_mode = InputMode::TablePicker;
                        state.filter_input.clear();
                        state.picker_selected = 0;
                    }
                    Action::ToggleFocus => state.focus_mode = !state.focus_mode,
                    _ => {}
                }
            }
        }

        Action::Escape => {
            state.input_mode = InputMode::Normal;
        }

        // Metadata loading
        Action::LoadMetadata => {
            if let Some(dsn) = &state.dsn {
                // Check cache first
                if let Some(cached) = metadata_cache.get(dsn).await {
                    state.metadata = Some(cached);
                    state.metadata_state = MetadataState::Loaded;
                } else {
                    state.metadata_state = MetadataState::Loading;

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
            if let Some(dsn) = &state.dsn {
                metadata_cache.invalidate(dsn).await;
                let _ = action_tx.send(Action::LoadMetadata).await;
            }
        }

        Action::MetadataLoaded(metadata) => {
            state.metadata = Some(*metadata);
            state.metadata_state = MetadataState::Loaded;
        }

        Action::MetadataFailed(error) => {
            state.metadata_state = MetadataState::Error(error);
        }

        Action::InvalidateCache => {
            if let Some(dsn) = &state.dsn {
                metadata_cache.invalidate(dsn).await;
            }
        }

        _ => {}
    }

    Ok(())
}

fn extract_database_name(dsn: &str) -> Option<String> {
    let name = PostgresAdapter::extract_database_name(dsn);
    if name == "unknown" { None } else { Some(name) }
}
