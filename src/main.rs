use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use color_eyre::eyre::Result;
use sabiql_app as app;
#[cfg(test)]
pub(crate) use sabiql_domain as domain;
use sabiql_infra as infra;
use sabiql_ui as ui;
use tokio::sync::mpsc;
use tokio::time::sleep_until;

mod error;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/render_snapshots/mod.rs"]
mod render_snapshots;

use crate::app::{
    Action, AppRuntime, AppState, CompletionEngine, DbCapabilities, EffectRunner, InputMode,
    StartupLoadError, TtlCache, handle_event, initialize_connection_state,
    next_animation_deadline,
};
use crate::app::ports::{
    ConnectionStore, DatabaseCapabilityProvider, PgServiceEntryReader,
};
use crate::app::AppServices;
use crate::infra::adapters::{
    ArboardClipboard, FileConfigWriter, FileQueryHistoryStore, FsErLogWriter, NativeFolderOpener,
    PgServiceFileReader, PostgresAdapter, TomlConnectionStore,
};
use crate::infra::config::project_root::{find_project_root, get_project_name};
use crate::infra::export::DotExporter;
use crate::ui::adapters::TuiAdapter;
use crate::ui::tui::TuiRunner;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    #[cfg(feature = "self-update")]
    /// Update sabiql to the latest compatible version
    Update,
    #[cfg(not(feature = "self-update"))]
    /// Self-update is disabled in this build
    #[command(hide = true)]
    Update,
}

#[tokio::main]
#[allow(
    clippy::print_stderr,
    reason = "CLI error output before TUI initialization"
)]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    error::install_hooks()?;

    let args = Args::parse();
    if matches!(args.command, Some(Command::Update)) {
        #[cfg(feature = "self-update")]
        {
            return run_update();
        }
        #[cfg(not(feature = "self-update"))]
        {
            eprintln!("{}", self_update_disabled_message());
            std::process::exit(1);
        }
    }

    let project_root = find_project_root()?;
    let project_name = get_project_name(&project_root);

    let (action_tx, mut action_rx) = mpsc::channel::<Action>(256);

    let adapter = Arc::new(PostgresAdapter::new());
    let metadata_cache = TtlCache::new(300);
    let completion_engine = RefCell::new(CompletionEngine::new());
    let connection_store = TomlConnectionStore::new()?;
    let all_profiles = connection_store.load_all();
    let connection_store = Arc::new(connection_store);

    let db_capabilities: DbCapabilities = adapter.capabilities().into();
    let pg_service_entry_reader: Arc<dyn PgServiceEntryReader> =
        Arc::new(PgServiceFileReader::new());

    let effect_runner = EffectRunner::builder()
        .metadata_provider(Arc::clone(&adapter) as _)
        .query_executor(Arc::clone(&adapter) as _)
        .dsn_builder(Arc::clone(&adapter) as _)
        .er_exporter(Arc::new(DotExporter::new()))
        .config_writer(Arc::new(FileConfigWriter::new()))
        .er_log_writer(Arc::new(FsErLogWriter))
        .connection_store(Arc::clone(&connection_store) as _)
        .pg_service_entry_reader(Arc::clone(&pg_service_entry_reader))
        .clipboard(Arc::new(ArboardClipboard))
        .folder_opener(Arc::new(NativeFolderOpener))
        .query_history_store(Arc::new(FileQueryHistoryStore::new()))
        .metadata_cache(metadata_cache.clone())
        .action_tx(action_tx.clone())
        .build();

    let services = AppServices {
        ddl_generator: Arc::clone(&adapter) as _,
        sql_dialect: Arc::clone(&adapter) as _,
        db_capabilities,
    };

    let mut state = AppState::new(project_name);
    let service_result = match &all_profiles {
        Ok(_) => Some(pg_service_entry_reader.read_services()),
        Err(_) => None,
    };
    if let Err(StartupLoadError::VersionMismatch { found, expected }) =
        initialize_connection_state(&mut state, all_profiles, service_result)
    {
        eprintln!(
            "Error: Configuration file version mismatch (found v{}, expected v{}).\n\
             Please delete {} and reconfigure.",
            found,
            expected,
            connection_store.storage_path().display()
        );
        std::process::exit(1);
    }

    let mut tui = TuiRunner::new()?;
    tui.enter()?;

    let initial_size = tui.terminal().size()?;
    state.ui.terminal_height = initial_size.height;
    let runtime = AppRuntime::new(&effect_runner, &completion_engine, &services);

    if state.session.dsn.is_some() && state.input_mode() == InputMode::Normal {
        dispatch_action(
            Action::TryConnect,
            &mut state,
            &mut tui,
            &runtime,
        )
        .await?;
    }

    let cache_cleanup_interval = Duration::from_secs(150);
    let mut last_cache_cleanup = Instant::now();

    loop {
        let now = Instant::now();
        let deadline = next_animation_deadline(&state, now);

        tokio::select! {
            Some(event) = tui.next_event() => {
                let action = handle_event(event.into(), &state, &services);
                if !action.is_none() {
                    drain_and_process_terminal_events(action, &mut state, &mut tui, &runtime).await?;
                }
            }
            Some(action) = action_rx.recv() => {
                dispatch_action(action, &mut state, &mut tui, &runtime).await?;
            }
            // Animation deadline reached (spinner, cursor blink, message timeout)
            () = async {
                match deadline {
                    Some(d) => sleep_until(d.into()).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                dispatch_action(Action::Render, &mut state, &mut tui, &runtime).await?;
            }
        }

        if let Some(debounce_until) = state.sql_modal.completion_debounce
            && Instant::now() >= debounce_until
        {
            state.sql_modal.completion_debounce = None;
            dispatch_action(
                Action::CompletionTrigger,
                &mut state,
                &mut tui,
                &runtime,
            )
            .await?;
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

async fn dispatch_action(
    action: Action,
    state: &mut AppState,
    tui: &mut TuiRunner,
    runtime: &AppRuntime<'_>,
) -> Result<()> {
    let mut tui_adapter = TuiAdapter::new(tui);
    runtime.dispatch(action, state, &mut tui_adapter).await
}

const MAX_DRAIN: usize = 32;

async fn drain_and_process_terminal_events(
    first_action: Action,
    state: &mut AppState,
    tui: &mut TuiRunner,
    runtime: &AppRuntime<'_>,
) -> Result<()> {
    if !first_action.is_scroll() {
        return dispatch_action(first_action, state, tui, runtime).await;
    }

    {
        let mut tui_adapter = TuiAdapter::new(tui);
        if runtime
            .flush_reduced(first_action, state, &mut tui_adapter)
            .await?
        {
            return Ok(());
        }
    }

    let mut drained = 0;
    while drained < MAX_DRAIN {
        let Some(event) = tui.try_next_event() else {
            break;
        };
        drained += 1;
        let action = handle_event(event.into(), state, runtime.services());
        if action.is_none() {
            continue;
        }

        if action.is_scroll() {
            let mut tui_adapter = TuiAdapter::new(tui);
            if runtime
                .flush_reduced(action, state, &mut tui_adapter)
                .await?
            {
                break;
            }
        } else {
            if state.render_dirty {
                state.clear_dirty();
                dispatch_action(Action::Render, state, tui, runtime).await?;
            }
            dispatch_action(action, state, tui, runtime).await?;
            if state.should_quit {
                return Ok(());
            }
        }
    }

    if state.render_dirty {
        state.clear_dirty();
        dispatch_action(Action::Render, state, tui, runtime).await?;
    }

    Ok(())
}
#[cfg(feature = "self-update")]
#[allow(clippy::print_stdout, reason = "CLI subcommand output, TUI not active")]
fn run_update() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("Current version: v{current}");
    println!("Checking for updates...");

    let status = self_update::backends::github::Update::configure()
        .repo_owner("riii111")
        .repo_name("sabiql")
        .bin_name("sabiql")
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(current)
        .build()?
        .update()?;

    if status.updated() {
        println!("Updated successfully: v{} -> {}", current, status.version());
    } else {
        println!("Already up to date (v{current}).");
    }

    Ok(())
}

#[cfg(not(feature = "self-update"))]
fn self_update_disabled_message() -> String {
    format!(
        "Self-update is not available in this build (v{}).\n\
         If installed via Homebrew: brew upgrade sabiql\n\
         If installed via cargo:    cargo install sabiql",
        env!("CARGO_PKG_VERSION")
    )
}
