use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use color_eyre::eyre::Result;
use tokio::sync::mpsc;
use tokio::time::sleep_until;

use sabiql::app::cmd::cache::TtlCache;
use sabiql::app::cmd::completion_engine::CompletionEngine;
use sabiql::app::cmd::effect::Effect;
use sabiql::app::cmd::render_schedule::next_animation_deadline;
use sabiql::app::cmd::runner::EffectRunner;
use sabiql::app::model::app_state::AppState;
use sabiql::app::model::shared::input_mode::InputMode;
use sabiql::app::ports::{
    ConnectionStore, ConnectionStoreError, ServiceFileError, ServiceFileReader,
};
use sabiql::app::services::AppServices;
use sabiql::app::update::action::Action;
use sabiql::app::update::reducer::reduce;
use sabiql::error;
use sabiql::infra::adapters::{
    ArboardClipboard, FileConfigWriter, FileQueryHistoryStore, FsErLogWriter, NativeFolderOpener,
    PgServiceFileReader, PostgresAdapter, TomlConnectionStore,
};
use sabiql::infra::config::project_root::{find_project_root, get_project_name};
use sabiql::infra::export::DotExporter;
use sabiql::ui::adapters::TuiAdapter;
use sabiql::ui::event::handlers::handle_event;
use sabiql::ui::tui::TuiRunner;

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

    let service_file_reader: Arc<dyn ServiceFileReader> = Arc::new(PgServiceFileReader::new());

    let effect_runner = EffectRunner::builder()
        .metadata_provider(Arc::clone(&adapter) as _)
        .query_executor(Arc::clone(&adapter) as _)
        .dsn_builder(Arc::clone(&adapter) as _)
        .er_exporter(Arc::new(DotExporter::new()))
        .config_writer(Arc::new(FileConfigWriter::new()))
        .er_log_writer(Arc::new(FsErLogWriter))
        .connection_store(Arc::clone(&connection_store) as _)
        .service_file_reader(Arc::clone(&service_file_reader))
        .clipboard(Arc::new(ArboardClipboard))
        .folder_opener(Arc::new(NativeFolderOpener))
        .query_history_store(Arc::new(FileQueryHistoryStore::new()))
        .metadata_cache(metadata_cache.clone())
        .action_tx(action_tx.clone())
        .build();

    let services = AppServices {
        ddl_generator: Arc::clone(&adapter) as _,
        sql_dialect: Arc::clone(&adapter) as _,
    };

    let mut state = AppState::new(project_name);

    match all_profiles {
        Ok(profiles) if profiles.is_empty() => {
            load_service_entries(&mut state, &*service_file_reader);
            if state.service_entries().is_empty() {
                state.connection_setup.is_first_run = true;
                state.modal.set_mode(InputMode::ConnectionSetup);
            } else {
                state.modal.set_mode(InputMode::ConnectionSelector);
                state.ui.set_connection_list_selection(Some(0));
            }
        }
        Ok(mut profiles) => {
            profiles.sort_by(|a, b| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            });
            state.set_connections(profiles);
            load_service_entries(&mut state, &*service_file_reader);

            state.modal.set_mode(InputMode::ConnectionSelector);
            state.ui.set_connection_list_selection(Some(0));
        }
        Err(ConnectionStoreError::VersionMismatch { found, expected }) => {
            eprintln!(
                "Error: Configuration file version mismatch (found v{}, expected v{}).\n\
                 Please delete {} and reconfigure.",
                found,
                expected,
                connection_store.storage_path().display()
            );
            std::process::exit(1);
        }
        Err(_) => {
            state.connection_setup.is_first_run = true;
            state.modal.set_mode(InputMode::ConnectionSetup);
        }
    }

    let mut tui = TuiRunner::new()?;
    tui.enter()?;

    let initial_size = tui.terminal().size()?;
    state.ui.terminal_height = initial_size.height;

    if state.session.dsn.is_some() && state.input_mode() == InputMode::Normal {
        process_action(
            Action::TryConnect,
            &mut state,
            &mut tui,
            &effect_runner,
            &completion_engine,
            &services,
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
                let action = handle_event(event, &state);
                if !action.is_none() {
                    drain_and_process_terminal_events(action, &mut state, &mut tui, &effect_runner, &completion_engine, &services).await?;
                }
            }
            Some(action) = action_rx.recv() => {
                process_action(action, &mut state, &mut tui, &effect_runner, &completion_engine, &services).await?;
            }
            // Animation deadline reached (spinner, cursor blink, message timeout)
            () = async {
                match deadline {
                    Some(d) => sleep_until(d.into()).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                process_action(Action::Render, &mut state, &mut tui, &effect_runner, &completion_engine, &services).await?;
            }
        }

        if let Some(debounce_until) = state.sql_modal.completion_debounce
            && Instant::now() >= debounce_until
        {
            state.sql_modal.completion_debounce = None;
            process_action(
                Action::CompletionTrigger,
                &mut state,
                &mut tui,
                &effect_runner,
                &completion_engine,
                &services,
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

async fn process_action(
    action: Action,
    state: &mut AppState,
    tui: &mut TuiRunner,
    effect_runner: &EffectRunner,
    completion_engine: &RefCell<CompletionEngine>,
    services: &AppServices,
) -> Result<()> {
    let now = Instant::now();
    let is_animation_tick = matches!(action, Action::Render);
    if is_animation_tick {
        state.clear_expired_timers(now);
    }
    let mut effects = reduce(state, action, now, services);
    if state.render_dirty {
        if !is_animation_tick {
            state.clear_expired_timers(now);
        }
        effects.push(Effect::Render);
    }
    flush_effects(
        effects,
        state,
        tui,
        effect_runner,
        completion_engine,
        services,
    )
    .await
}

#[allow(
    clippy::print_stderr,
    reason = "last-resort fallback when effect dispatch exceeds recursion limit"
)]
async fn flush_effects(
    effects: Vec<Effect>,
    state: &mut AppState,
    tui: &mut TuiRunner,
    effect_runner: &EffectRunner,
    completion_engine: &RefCell<CompletionEngine>,
    services: &AppServices,
) -> Result<()> {
    let mut tui_adapter = TuiAdapter::new(tui);
    let mut pending = effect_runner
        .run(
            effects,
            &mut tui_adapter,
            state,
            completion_engine,
            services,
        )
        .await?;
    state.clear_dirty();

    const MAX_DEPTH: usize = 16;
    let mut depth = 0;
    while !pending.is_empty() && depth < MAX_DEPTH {
        depth += 1;
        let mut next = Vec::new();
        for action in pending {
            let now = Instant::now();
            let mut effects = reduce(state, action, now, services);
            if state.render_dirty {
                state.clear_expired_timers(now);
                effects.push(Effect::Render);
            }
            let mut tui_adapter = TuiAdapter::new(tui);
            next.extend(
                effect_runner
                    .run(
                        effects,
                        &mut tui_adapter,
                        state,
                        completion_engine,
                        services,
                    )
                    .await?,
            );
            state.clear_dirty();
        }
        pending = next;
    }
    if depth >= MAX_DEPTH && !pending.is_empty() {
        eprintln!(
            "DispatchActions recursion depth exceeded ({MAX_DEPTH}), \
             falling back to channel for {} remaining actions",
            pending.len()
        );
        for action in pending {
            if let Err(e) = effect_runner.action_tx().try_send(action) {
                eprintln!("DispatchActions fallback: channel full, dropping action: {e}");
            }
        }
    }
    Ok(())
}

const MAX_DRAIN: usize = 32;

async fn drain_and_process_terminal_events(
    first_action: Action,
    state: &mut AppState,
    tui: &mut TuiRunner,
    effect_runner: &EffectRunner,
    completion_engine: &RefCell<CompletionEngine>,
    services: &AppServices,
) -> Result<()> {
    if !first_action.is_scroll() {
        return process_action(
            first_action,
            state,
            tui,
            effect_runner,
            completion_engine,
            services,
        )
        .await;
    }

    let now = Instant::now();
    let mut effects = reduce(state, first_action, now, services);
    if !effects.is_empty() {
        if state.render_dirty {
            state.clear_expired_timers(now);
            effects.push(Effect::Render);
        }
        return flush_effects(
            effects,
            state,
            tui,
            effect_runner,
            completion_engine,
            services,
        )
        .await;
    }

    let mut drained = 0;
    while drained < MAX_DRAIN {
        let Some(event) = tui.try_next_event() else {
            break;
        };
        drained += 1;
        let action = handle_event(event, state);
        if action.is_none() {
            continue;
        }

        if action.is_scroll() {
            let now = Instant::now();
            let mut effects = reduce(state, action, now, services);
            if !effects.is_empty() {
                if state.render_dirty {
                    state.clear_expired_timers(now);
                    effects.push(Effect::Render);
                }
                flush_effects(
                    effects,
                    state,
                    tui,
                    effect_runner,
                    completion_engine,
                    services,
                )
                .await?;
                break;
            }
        } else {
            if state.render_dirty {
                state.clear_dirty();
                process_action(
                    Action::Render,
                    state,
                    tui,
                    effect_runner,
                    completion_engine,
                    services,
                )
                .await?;
            }
            process_action(
                action,
                state,
                tui,
                effect_runner,
                completion_engine,
                services,
            )
            .await?;
            if state.should_quit {
                return Ok(());
            }
        }
    }

    if state.render_dirty {
        state.clear_dirty();
        process_action(
            Action::Render,
            state,
            tui,
            effect_runner,
            completion_engine,
            services,
        )
        .await?;
    }

    Ok(())
}

fn load_service_entries(state: &mut AppState, reader: &dyn ServiceFileReader) {
    match reader.read_services() {
        Ok((services, path)) if !services.is_empty() => {
            state.set_service_entries(services);
            state.runtime.service_file_path = Some(path);
        }
        Ok(_) | Err(ServiceFileError::NotFound(_)) => {}
        Err(e) => {
            state.messages.set_error(e.to_string());
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn no_subcommand_returns_none() {
        let args = Args::parse_from(["sabiql"]);
        assert!(args.command.is_none());
    }

    #[test]
    #[cfg(feature = "self-update")]
    fn update_subcommand_is_recognized() {
        let args = Args::parse_from(["sabiql", "update"]);
        assert!(matches!(args.command, Some(Command::Update)));
    }

    #[test]
    #[cfg(not(feature = "self-update"))]
    fn update_subcommand_available_but_self_update_disabled() {
        let args = Args::parse_from(["sabiql", "update"]);
        assert!(matches!(args.command, Some(Command::Update)));
    }

    #[test]
    #[cfg(not(feature = "self-update"))]
    fn disabled_message_contains_version_and_upgrade_guidance() {
        let msg = self_update_disabled_message();
        assert!(msg.contains(env!("CARGO_PKG_VERSION")));
        assert!(msg.contains("brew upgrade sabiql"));
        assert!(msg.contains("cargo install sabiql"));
    }
}
