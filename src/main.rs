#![allow(
    clippy::disallowed_methods,
    reason = "the main loop is the time source: it reads the clock and injects `now` into reducers"
)]
#![cfg_attr(
    test,
    allow(
        unreachable_pub,
        reason = "test support visibility is excluded from production API measurement"
    )
)]

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

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

use sabiql_app::cmd::cli_sqlite::{
    CliSqliteTarget, activate_cli_sqlite_connection, resolve_cli_sqlite_target,
};
use sabiql_app::cmd::completion_engine::CompletionEngine;
use sabiql_app::cmd::effect::Effect;
use sabiql_app::cmd::render_schedule::next_animation_deadline;
use sabiql_app::cmd::runner::{ConnectionDeps, EffectRunner, ErDeps, QueryDeps, UtilityDeps};
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::shared::input_mode::InputMode;
use sabiql_app::ports::outbound::{
    AppSettings, ConnectionStore, ConnectionStoreError, MySqlConnectionProbe, PgServiceEntryReader,
    ServiceFileError, SqliteDiagnosticsProvider,
};
use sabiql_app::services::AppServices;
use sabiql_app::update::action::Action;
use sabiql_app::update::input::handle_event;
use sabiql_app::update::reducer::reduce;
use sabiql_infra::adapters::mysql::MySqlAdapter;
use sabiql_infra::adapters::{
    ArboardClipboard, CsvCachedResultExporter, DbAdapterRegistry, FileConfigWriter,
    FileQueryHistoryStore, FsErLogWriter, FsSqlitePathValidator, NativeFolderOpener,
    PgServiceFileReader, SqliteAdapter, TomlConnectionStore, TomlSettingsStore,
};
use sabiql_infra::config::project_root::{find_project_root, get_project_name};
use sabiql_infra::export::DotExporter;
use sabiql_ui::adapters::TuiAdapter;
use sabiql_ui::tui::TuiRunner;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// SQLite database file path or sqlite:// DSN
    database: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    #[cfg(feature = "self-update")]
    /// Update sabiql to the latest stable version
    Update,
    #[cfg(not(feature = "self-update"))]
    /// Self-update is disabled in this build
    #[command(hide = true)]
    Update,
}

#[cfg(feature = "self-update")]
fn latest_stable_release<'a>(
    current_version: &str,
    releases: &'a [self_update::update::Release],
) -> Option<&'a self_update::update::Release> {
    releases
        .iter()
        .filter(|release| !release.version.contains('-'))
        .filter(|release| {
            self_update::version::bump_is_greater(current_version, &release.version)
                .unwrap_or(false)
        })
        .reduce(|latest, release| {
            if self_update::version::bump_is_greater(&latest.version, &release.version)
                .unwrap_or(false)
            {
                release
            } else {
                latest
            }
        })
}

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

    let cli_sqlite = resolve_cli_database(args.database)?;
    let project_root = find_project_root()?;
    let project_name = get_project_name(&project_root);
    let infrastructure = build_infrastructure()?;
    let app_settings = infrastructure.settings_store.load().unwrap_or_default();
    let state = initialize_state(
        project_name,
        app_settings,
        cli_sqlite.as_ref(),
        &infrastructure,
    )?;
    let runtime = Runtime::new(state, infrastructure)?;
    Box::pin(runtime.run()).await
}

fn resolve_cli_database(database: Option<String>) -> Result<Option<CliSqliteTarget>> {
    Ok(database
        .map(|database| resolve_cli_sqlite_target(&database, &FsSqlitePathValidator))
        .transpose()?)
}

struct Infrastructure {
    adapter_registry: Arc<DbAdapterRegistry>,
    connection_store: Arc<TomlConnectionStore>,
    settings_store: Arc<TomlSettingsStore>,
    pg_service_entry_reader: Arc<dyn PgServiceEntryReader>,
}

fn build_infrastructure() -> Result<Infrastructure> {
    Ok(Infrastructure {
        adapter_registry: Arc::new(DbAdapterRegistry::new()),
        connection_store: Arc::new(TomlConnectionStore::new()?),
        settings_store: Arc::new(TomlSettingsStore::new()?),
        pg_service_entry_reader: Arc::new(PgServiceFileReader::new()),
    })
}

#[allow(
    clippy::exit,
    clippy::print_stderr,
    reason = "configuration errors are reported before TUI initialization"
)]
fn initialize_state(
    project_name: String,
    app_settings: AppSettings,
    cli_sqlite: Option<&CliSqliteTarget>,
    infrastructure: &Infrastructure,
) -> Result<AppState> {
    let mut state = AppState::new(project_name);
    apply_app_settings(&mut state, app_settings);

    match infrastructure.connection_store.load_all() {
        Ok(profiles) if profiles.is_empty() => {
            load_service_entries(&mut state, infrastructure.pg_service_entry_reader.as_ref());
            configure_initial_connection_view(&mut state, cli_sqlite.is_some(), false);
        }
        Ok(mut profiles) => {
            profiles.sort_by(|a, b| {
                a.display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase())
            });
            state.set_connections(profiles);
            load_service_entries(&mut state, infrastructure.pg_service_entry_reader.as_ref());
            configure_initial_connection_view(&mut state, cli_sqlite.is_some(), true);
        }
        Err(ConnectionStoreError::VersionMismatch { found, expected }) if cli_sqlite.is_none() => {
            eprintln!(
                "Error: Configuration file version mismatch (found v{}, expected v{}).\n\
                 Please delete {} and reconfigure.",
                found,
                expected,
                infrastructure.connection_store.storage_path().display()
            );
            std::process::exit(1);
        }
        Err(_) if cli_sqlite.is_none() => {
            state.connection_setup.set_first_run(true);
            state.modal.set_mode(InputMode::ConnectionSetup);
        }
        Err(_) => {}
    }

    if let Some(target) = cli_sqlite {
        activate_cli_sqlite_connection(&mut state, target, &FsSqlitePathValidator)?;
    }

    Ok(state)
}

fn apply_app_settings(state: &mut AppState, app_settings: AppSettings) {
    state.ui.set_theme(app_settings.theme_id);
    state
        .settings
        .load_keymap_preset(app_settings.keymap_preset);
    state.settings.load_er_browser(app_settings.er_browser);
}

fn configure_initial_connection_view(
    state: &mut AppState,
    has_cli_database: bool,
    has_saved_profiles: bool,
) {
    if has_cli_database {
        return;
    }

    if !has_saved_profiles && state.service_entries().is_empty() {
        state.connection_setup.set_first_run(true);
        state.modal.set_mode(InputMode::ConnectionSetup);
    } else {
        state.modal.set_mode(InputMode::ConnectionSelector);
        state.ui.set_connection_list_selection(Some(0));
    }
}

const MAX_DEPTH: usize = 16;
const MAX_DRAIN: usize = 32;

struct Runtime {
    state: AppState,
    tui: TuiRunner,
    action_rx: mpsc::Receiver<Action>,
    effect_runner: EffectRunner,
    completion_engine: RefCell<CompletionEngine>,
    services: AppServices,
}

impl Runtime {
    fn new(state: AppState, infrastructure: Infrastructure) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::channel::<Action>(256);
        let adapter_registry = infrastructure.adapter_registry;
        let mysql_connection_probe: Arc<dyn MySqlConnectionProbe> = Arc::new(MySqlAdapter::new());
        let sqlite_diagnostics: Arc<dyn SqliteDiagnosticsProvider> = Arc::new(SqliteAdapter::new());
        let completion_engine = RefCell::new(CompletionEngine::new());
        let effect_runner = EffectRunner::new(
            Arc::clone(&adapter_registry) as _,
            ConnectionDeps {
                dsn_builder: Arc::clone(&adapter_registry) as _,
                mysql_connection_probe,
                connection_store: Arc::clone(&infrastructure.connection_store) as _,
                pg_service_entry_reader: Arc::clone(&infrastructure.pg_service_entry_reader),
                sqlite_path_validator: Arc::new(FsSqlitePathValidator),
            },
            QueryDeps {
                query_executor: Arc::clone(&adapter_registry) as _,
                query_history_store: Arc::new(FileQueryHistoryStore::new()),
                sqlite_diagnostics,
                cached_result_exporter: Arc::new(CsvCachedResultExporter),
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
            Arc::clone(&infrastructure.settings_store) as _,
            action_tx,
        );
        let services = AppServices {
            ddl_generator: Arc::clone(&adapter_registry) as _,
            dsn_builder: Arc::clone(&adapter_registry) as _,
        };

        Ok(Self {
            state,
            tui: TuiRunner::new()?,
            action_rx,
            effect_runner,
            completion_engine,
            services,
        })
    }

    async fn run(mut self) -> Result<()> {
        self.tui.enter()?;

        let initial_size = self.tui.terminal().size()?;
        self.state.ui.set_terminal_width(initial_size.width);
        self.state.ui.set_terminal_height(initial_size.height);

        if self.state.session.dsn().is_some() && self.state.input_mode() == InputMode::Normal {
            self.process_action(Action::TryConnect).await?;
        }

        loop {
            let now = Instant::now();
            let deadline = next_animation_deadline(&self.state, now);

            tokio::select! {
                event = self.tui.next_event() => {
                    let event = event?;
                    let action = handle_event(event, &self.state);
                    if !action.is_none() {
                        self.process_terminal_event_burst(action).await?;
                    }
                }
                Some(action) = self.action_rx.recv() => {
                    self.process_action(action).await?;
                }
                // Animation deadline reached (spinner, cursor blink, message timeout)
                () = async {
                    match deadline {
                        Some(d) => sleep_until(d.into()).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    self.process_action(Action::Render).await?;
                }
            }

            if let Some(debounce_until) = self.state.sql_modal.completion_debounce()
                && Instant::now() >= debounce_until
            {
                self.state.sql_modal.consume_completion_debounce();
                self.process_action(Action::CompletionRequest).await?;
            }

            if self.state.should_quit {
                break;
            }
        }

        self.tui.exit()?;
        Ok(())
    }

    async fn process_action(&mut self, action: Action) -> Result<()> {
        let now = Instant::now();
        let is_animation_tick = matches!(action, Action::Render);
        if is_animation_tick {
            self.state.clear_expired_timers(now);
        }
        let mut effects = reduce(&mut self.state, action, now, &self.services);
        if is_animation_tick {
            if self.state.render_dirty {
                effects.push(Effect::Render);
            }
        } else {
            self.append_render_if_dirty(&mut effects, now);
        }
        self.flush_effects(effects).await
    }

    fn append_render_if_dirty(&mut self, effects: &mut Vec<Effect>, now: Instant) {
        if self.state.render_dirty {
            self.state.clear_expired_timers(now);
            effects.push(Effect::Render);
        }
    }

    async fn run_effects(&mut self, effects: Vec<Effect>) -> Result<Vec<Action>> {
        let mut tui_adapter = TuiAdapter::new(&mut self.tui);
        let pending = self
            .effect_runner
            .execute_effects(
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
                self.append_render_if_dirty(&mut effects, now);
                next.extend(self.run_effects(effects).await?);
            }
            pending = next;
        }
        if depth >= MAX_DEPTH && !pending.is_empty() {
            dispatch_overflow_fallback(&mut self.state, self.effect_runner.action_tx(), pending);
            // Render immediately so the overflow error is visible before the next
            // event-loop pass; errors do not have an expiry wake-up anymore.
            self.run_effects(vec![Effect::Render]).await?;
        }
        Ok(())
    }

    async fn process_terminal_event_burst(&mut self, first_action: Action) -> Result<()> {
        if !first_action.is_scroll() {
            self.state.messages.clear_error();
            return self.process_action(first_action).await;
        }

        self.state.messages.clear_error();
        let now = Instant::now();
        let mut effects = reduce(&mut self.state, first_action, now, &self.services);
        if !effects.is_empty() {
            self.append_render_if_dirty(&mut effects, now);
            return self.flush_effects(effects).await;
        }

        // Effect-free scroll reduces only mutate state; defer the render to the
        // end of the burst so N input events produce one draw, not N.
        let mut drained = 0;
        while drained < MAX_DRAIN {
            let Some(event) = self.tui.try_next_event()? else {
                break;
            };
            drained += 1;
            let action = handle_event(event, &self.state);
            if action.is_none() {
                continue;
            }

            if action.is_scroll() {
                self.state.messages.clear_error();
                let now = Instant::now();
                let mut effects = reduce(&mut self.state, action, now, &self.services);
                if !effects.is_empty() {
                    self.append_render_if_dirty(&mut effects, now);
                    self.flush_effects(effects).await?;
                    break;
                }
            } else {
                if self.state.render_dirty {
                    self.state.clear_dirty();
                    self.process_action(Action::Render).await?;
                }
                self.state.messages.clear_error();
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
    state.messages.set_error(message);
}

fn load_service_entries(state: &mut AppState, reader: &dyn PgServiceEntryReader) {
    match reader.read_services() {
        Ok((services, path)) if !services.is_empty() => {
            state.set_service_entries(services);
            state.set_service_file_path(Some(path));
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

    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner("riii111")
        .repo_name("sabiql")
        .build()?
        .fetch()?;
    let Some(latest) = latest_stable_release(current, &releases) else {
        println!("Already up to date (v{current}).");
        return Ok(());
    };
    let target_version = format!("v{}", latest.version);

    let status = self_update::backends::github::Update::configure()
        .repo_owner("riii111")
        .repo_name("sabiql")
        .bin_name("sabiql")
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(current)
        .target_version_tag(&target_version)
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
