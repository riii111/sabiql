mod app;
mod domain;
mod error;
mod infra;
mod ui;

use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use app::action::Action;
use app::completion::CompletionEngine;
use app::effect_runner::EffectRunner;
use app::reducer::reduce;
use app::state::AppState;
use infra::adapters::PostgresAdapter;
use infra::cache::TtlCache;
use infra::config::{
    dbx_toml::DbxConfig,
    project_root::{find_project_root, get_project_name},
};
use infra::export::DotExporter;
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

    let (action_tx, mut action_rx) = mpsc::channel::<Action>(256);

    let adapter = Arc::new(PostgresAdapter::new());
    let metadata_cache = TtlCache::new(300);
    let completion_engine = RefCell::new(CompletionEngine::new());

    let effect_runner = EffectRunner::new(
        Arc::clone(&adapter) as _,
        Arc::clone(&adapter) as _,
        Arc::new(DotExporter::new()),
        metadata_cache.clone(),
        action_tx.clone(),
    );

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
                let now = Instant::now();
                let effects = reduce(&mut state, action, now);
                effect_runner.run(effects, &mut tui, &mut state, &completion_engine).await?;
            }
        }

        // Handle completion debounce
        if let Some(debounce_until) = state.sql_modal.completion_debounce
            && Instant::now() >= debounce_until
        {
            state.sql_modal.completion_debounce = None;
            let _ = action_tx.send(Action::CompletionTrigger).await;
        }

        // Periodic cache cleanup
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

fn extract_database_name(dsn: &str) -> Option<String> {
    let name = PostgresAdapter::extract_database_name(dsn);
    if name == "unknown" { None } else { Some(name) }
}
