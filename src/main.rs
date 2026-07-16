#![allow(
    clippy::disallowed_methods,
    reason = "the main loop is the time source: it reads the clock and injects `now` into reducers"
)]

use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use color_eyre::eyre::Result;
use tokio::sync::mpsc;
use tokio::time::sleep_until;

mod panic_hooks;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/render_snapshots/mod.rs"]
mod render_snapshots;

use sabiql_app::cmd::cache::TtlCache;
use sabiql_app::cmd::completion_engine::CompletionEngine;
use sabiql_app::cmd::effect::Effect;
use sabiql_app::cmd::render_schedule::next_animation_deadline;
use sabiql_app::cmd::runner::{
    ConnectionDeps, EffectRunner, ErDeps, QueryDeps, SettingsDeps, UtilityDeps,
};
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::shared::db_capabilities::DbCapabilities;
use sabiql_app::model::shared::input_mode::InputMode;
use sabiql_app::ports::outbound::{
    ConnectionStore, ConnectionStoreError, DatabaseCapabilityProvider, DdlGenerator,
    PgServiceEntryReader, ServiceFileError, SettingsStore, SqlDialect,
};
use sabiql_app::services::AppServices;
use sabiql_app::update::action::Action;
use sabiql_app::update::input::handle_event;
use sabiql_app::update::reducer::reduce;
use sabiql_infra::adapters::{
    ArboardClipboard, FileConfigWriter, FileQueryHistoryStore, FsErLogWriter, NativeFolderOpener,
    PgServiceFileReader, PostgresAdapter, TomlConnectionStore, TomlSettingsStore,
};
use sabiql_infra::config::project_root::{find_project_root, get_project_name};
use sabiql_infra::export::DotExporter;
use sabiql_ui::adapters::TuiAdapter;
use sabiql_ui::tui::TuiRunner;

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

const MAX_DEPTH: usize = 16;
const MAX_DRAIN: usize = 32;

#[tokio::main]
#[allow(
    clippy::print_stderr,
    reason = "CLI error output before TUI initialization"
)]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    panic_hooks::install_hooks()?;

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
    let settings_store = TomlSettingsStore::new()?;
    let app_settings = settings_store.load().unwrap_or_default();
    let connection_store = Arc::new(connection_store);
    let settings_store = Arc::new(settings_store);

    let db_capabilities: DbCapabilities = adapter.capabilities().into();
    let pg_service_entry_reader: Arc<dyn PgServiceEntryReader> =
        Arc::new(PgServiceFileReader::new());

    let effect_runner = EffectRunner::new(
        Arc::clone(&adapter) as _,
        ConnectionDeps {
            dsn_builder: Arc::clone(&adapter) as _,
            connection_store: Arc::clone(&connection_store) as _,
            pg_service_entry_reader: Some(Arc::clone(&pg_service_entry_reader)),
        },
        QueryDeps {
            query_executor: Arc::clone(&adapter) as _,
            query_history_store: Arc::new(FileQueryHistoryStore::new()),
        },
        ErDeps {
            er_exporter: Arc::new(DotExporter::new()),
            config_writer: Arc::new(FileConfigWriter::new()),
            er_log_writer: Arc::new(FsErLogWriter),
        },
        UtilityDeps {
            clipboard: Arc::new(ArboardClipboard),
            folder_opener: Arc::new(NativeFolderOpener),
        },
        SettingsDeps {
            settings_store: Arc::clone(&settings_store) as _,
        },
        metadata_cache.clone(),
        action_tx.clone(),
    );

    let ddl_generator: Arc<dyn DdlGenerator> = adapter.clone();
    let sql_dialect: Arc<dyn SqlDialect> = adapter.clone();
    let services = AppServices {
        ddl_generator,
        sql_dialect,
        dsn_builder: adapter.clone(),
        db_capabilities,
    };

    let mut state = AppState::new(project_name);
    state.ui.set_theme(app_settings.theme_id);
    state
        .settings
        .load_keymap_preset(app_settings.keymap_preset);
    state.settings.load_er_browser(app_settings.er_browser);

    let all_profiles = connection_store.load_all();
    match all_profiles {
        Ok(profiles) if profiles.is_empty() => {
            load_service_entries(&mut state, pg_service_entry_reader.as_ref());
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
            load_service_entries(&mut state, pg_service_entry_reader.as_ref());

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
    state.ui.terminal_width = initial_size.width;
    state.ui.terminal_height = initial_size.height;

    let mut runtime = Runtime {
        state,
        tui,
        effect_runner,
        completion_engine,
        services,
    };

    if runtime.state.session.dsn.is_some() && runtime.state.input_mode() == InputMode::Normal {
        runtime.process_action(Action::TryConnect).await?;
    }

    let cache_cleanup_interval = Duration::from_secs(150);
    let mut last_cache_cleanup = Instant::now();

    loop {
        let now = Instant::now();
        let deadline = next_animation_deadline(&runtime.state, now);

        tokio::select! {
            Some(event) = runtime.tui.next_event() => {
                let action = handle_event(event, &runtime.state, &runtime.services);
                if !action.is_none() {
                    runtime.process_terminal_event_burst(action).await?;
                }
            }
            Some(action) = action_rx.recv() => {
                runtime.process_action(action).await?;
            }
            // Animation deadline reached (spinner, cursor blink, message timeout)
            () = async {
                match deadline {
                    Some(d) => sleep_until(d.into()).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                runtime.process_action(Action::Render).await?;
            }
        }

        if let Some(debounce_until) = runtime.state.sql_modal.completion_debounce()
            && Instant::now() >= debounce_until
        {
            runtime.state.sql_modal.consume_completion_debounce();
            runtime.process_action(Action::CompletionRequest).await?;
        }

        if last_cache_cleanup.elapsed() >= cache_cleanup_interval {
            metadata_cache.cleanup_expired().await;
            last_cache_cleanup = Instant::now();
        }

        if runtime.state.should_quit {
            break;
        }
    }

    runtime.tui.exit()?;
    Ok(())
}

struct Runtime {
    state: AppState,
    tui: TuiRunner,
    effect_runner: EffectRunner,
    completion_engine: RefCell<CompletionEngine>,
    services: AppServices,
}

impl Runtime {
    async fn process_action(&mut self, action: Action) -> Result<()> {
        let now = Instant::now();
        let is_animation_tick = matches!(action, Action::Render);
        if is_animation_tick {
            self.state.clear_expired_timers(now);
        }
        let mut effects = reduce(&mut self.state, action, now, &self.services);
        if self.state.render_dirty {
            if !is_animation_tick {
                self.state.clear_expired_timers(now);
            }
            effects.push(Effect::Render);
        }
        self.flush_effects(effects).await
    }

    async fn run_effects(&mut self, effects: Vec<Effect>) -> Result<Vec<Action>> {
        let mut tui_adapter = TuiAdapter::new(&mut self.tui);
        let pending = self
            .effect_runner
            .run(
                effects,
                &mut tui_adapter,
                &mut self.state,
                &self.completion_engine,
                &self.services,
            )
            .await?;
        self.state.clear_dirty();
        Ok(pending)
    }

    async fn flush_effects(&mut self, effects: Vec<Effect>) -> Result<()> {
        let mut pending = self.run_effects(effects).await?;

        let mut depth = 0;
        while !pending.is_empty() && depth < MAX_DEPTH {
            depth += 1;
            let mut next = Vec::new();
            for action in pending {
                let now = Instant::now();
                let mut effects = reduce(&mut self.state, action, now, &self.services);
                if self.state.render_dirty {
                    self.state.clear_expired_timers(now);
                    effects.push(Effect::Render);
                }
                next.extend(self.run_effects(effects).await?);
            }
            pending = next;
        }
        if depth >= MAX_DEPTH && !pending.is_empty() {
            dispatch_overflow_fallback(
                &mut self.state,
                self.effect_runner.action_tx(),
                pending,
                Instant::now(),
            );
            // Render immediately: the main loop's next wakeup is the message expiry
            // itself, so without this draw the message would never become visible.
            self.run_effects(vec![Effect::Render]).await?;
        }
        Ok(())
    }

    async fn process_terminal_event_burst(&mut self, first_action: Action) -> Result<()> {
        if !first_action.is_scroll() {
            return self.process_action(first_action).await;
        }

        let now = Instant::now();
        let mut effects = reduce(&mut self.state, first_action, now, &self.services);
        if !effects.is_empty() {
            if self.state.render_dirty {
                self.state.clear_expired_timers(now);
                effects.push(Effect::Render);
            }
            return self.flush_effects(effects).await;
        }

        // Keep effect-free scroll actions in state so a terminal burst produces
        // one render instead of one render per input event.
        let mut drained = 0;
        while drained < MAX_DRAIN {
            let Some(event) = self.tui.try_next_event() else {
                break;
            };
            drained += 1;
            let action = handle_event(event, &self.state, &self.services);
            if action.is_none() {
                continue;
            }

            if action.is_scroll() {
                let now = Instant::now();
                let mut effects = reduce(&mut self.state, action, now, &self.services);
                if !effects.is_empty() {
                    if self.state.render_dirty {
                        self.state.clear_expired_timers(now);
                        effects.push(Effect::Render);
                    }
                    self.flush_effects(effects).await?;
                    break;
                }
            } else {
                if self.state.render_dirty {
                    self.state.clear_dirty();
                    self.process_action(Action::Render).await?;
                }
                self.process_action(action).await?;
                if self.state.should_quit {
                    return Ok(());
                }
            }
        }

        if self.state.render_dirty {
            self.state.clear_dirty();
            self.process_action(Action::Render).await?;
        }

        Ok(())
    }
}

/// Last-resort handling when DispatchActions recursion exceeds the depth
/// limit: re-queue through the action channel and surface the failure as a
/// UI error message (stderr would corrupt the TUI-owned screen).
fn dispatch_overflow_fallback(
    state: &mut AppState,
    action_tx: &mpsc::Sender<Action>,
    pending: Vec<Action>,
    now: Instant,
) {
    let deferred = pending.len();
    let mut dropped = 0usize;
    for action in pending {
        if action_tx.try_send(action).is_err() {
            dropped += 1;
        }
    }
    let message = if dropped > 0 {
        format!(
            "Internal error: action dispatch depth exceeded ({MAX_DEPTH}); {dropped} actions dropped"
        )
    } else {
        format!(
            "Internal error: action dispatch depth exceeded ({MAX_DEPTH}); {deferred} actions deferred"
        )
    };
    state.messages.set_error_at(message, now);
}

fn load_service_entries(state: &mut AppState, reader: &dyn PgServiceEntryReader) {
    match reader.read_services() {
        Ok((services, path)) if !services.is_empty() => {
            state.set_service_entries(services);
            state.runtime.service_file_path = Some(path);
        }
        Ok(_) | Err(ServiceFileError::NotFound(_)) => {}
        Err(e) => {
            state.messages.set_error_at(e.to_string(), Instant::now());
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
