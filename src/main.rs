use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use color_eyre::eyre::Result;
use tokio::sync::mpsc;
use tokio::time::sleep_until;

use sabiql::app::action::Action;
use sabiql::app::cache::TtlCache;
use sabiql::app::completion::CompletionEngine;
use sabiql::app::effect::Effect;
use sabiql::app::effect_runner::EffectRunner;
use sabiql::app::input_mode::InputMode;
use sabiql::app::ports::{ConnectionStore, ConnectionStoreError};
use sabiql::app::reducer::reduce;
use sabiql::app::render_schedule::next_animation_deadline;
use sabiql::app::state::AppState;
use sabiql::error;
use sabiql::infra::adapters::{FileConfigWriter, PostgresAdapter, TomlConnectionStore};
use sabiql::infra::config::project_root::{find_project_root, get_project_name};
use sabiql::infra::export::DotExporter;
use sabiql::ui::adapters::TuiAdapter;
use sabiql::ui::event::handler::handle_event;
use sabiql::ui::tui::TuiRunner;

/// CLI arguments (empty, but needed for --help and --version)
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    error::install_hooks()?;

    Args::parse(); // --help, --version
    let project_root = find_project_root()?;
    let project_name = get_project_name(&project_root);

    let (action_tx, mut action_rx) = mpsc::channel::<Action>(256);

    let adapter = Arc::new(PostgresAdapter::new());
    let metadata_cache = TtlCache::new(300);
    let completion_engine = RefCell::new(CompletionEngine::new());
    let connection_store = TomlConnectionStore::new()?;
    let loaded_profile = connection_store.load();
    let connection_store = Arc::new(connection_store);

    let effect_runner = EffectRunner::new(
        Arc::clone(&adapter) as _,
        Arc::clone(&adapter) as _,
        Arc::new(DotExporter::new()),
        Arc::new(FileConfigWriter::new()),
        Arc::clone(&connection_store) as _,
        metadata_cache.clone(),
        action_tx.clone(),
    );

    let mut state = AppState::new(project_name);

    match loaded_profile {
        Ok(Some(profile)) => {
            state.runtime.dsn = Some(profile.to_dsn());
            state.runtime.database_name = Some(profile.database.clone());
            state.runtime.active_connection_name = Some(profile.display_name().to_string());
            state.connection_setup.name = profile.name.as_str().to_string();
            state.connection_setup.host = profile.host;
            state.connection_setup.port = profile.port.to_string();
            state.connection_setup.database = profile.database;
            state.connection_setup.user = profile.username;
            state.connection_setup.password = profile.password;
            state.connection_setup.ssl_mode = profile.ssl_mode;
            state.connection_setup.is_first_run = false;
        }
        Ok(None) => {
            state.connection_setup.is_first_run = true;
            state.ui.input_mode = InputMode::ConnectionSetup;
        }
        Err(ConnectionStoreError::VersionMismatch { found, expected }) => {
            eprintln!(
                "Error: Configuration file version mismatch (found v{}, expected v{}).\n\
                 Please delete ~/.config/sabiql/connections.toml and reconfigure.",
                found, expected
            );
            std::process::exit(1);
        }
        Err(_) => {
            state.connection_setup.is_first_run = true;
            state.ui.input_mode = InputMode::ConnectionSetup;
        }
    }

    state.action_tx = Some(action_tx.clone());

    let mut tui = TuiRunner::new()?;
    tui.enter()?;

    let initial_size = tui.terminal().size()?;
    state.ui.terminal_height = initial_size.height;

    // TryConnect is idempotent, so safe even if called multiple times
    if state.runtime.dsn.is_some() && state.ui.input_mode == InputMode::Normal {
        let _ = action_tx.send(Action::TryConnect).await;
    }

    let cache_cleanup_interval = Duration::from_secs(150);
    let mut last_cache_cleanup = Instant::now();

    loop {
        let now = Instant::now();
        let deadline = next_animation_deadline(&state, now);

        tokio::select! {
            Some(event) = tui.next_event() => {
                let action = handle_event(event, &state);
                if !action.is_none() {
                    let _ = action_tx.send(action).await;
                }
            }
            Some(action) = action_rx.recv() => {
                let now = Instant::now();
                let mut effects = reduce(&mut state, action, now);

                if state.render_dirty {
                    state.clear_expired_timers(now);
                    effects.push(Effect::Render);
                }

                let mut tui_adapter = TuiAdapter::new(&mut tui);
                effect_runner.run(effects, &mut tui_adapter, &mut state, &completion_engine).await?;
                state.clear_dirty();
            }
            // Animation deadline reached (spinner, cursor blink, message timeout)
            _ = async {
                match deadline {
                    Some(d) => sleep_until(d.into()).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                let now = Instant::now();
                state.clear_expired_timers(now);
                let effects = reduce(&mut state, Action::Render, now);
                let mut tui_adapter = TuiAdapter::new(&mut tui);
                effect_runner.run(effects, &mut tui_adapter, &mut state, &completion_engine).await?;
                state.clear_dirty();
            }
        }

        if let Some(debounce_until) = state.sql_modal.completion_debounce
            && Instant::now() >= debounce_until
        {
            state.sql_modal.completion_debounce = None;
            let _ = action_tx.send(Action::CompletionTrigger).await;
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
