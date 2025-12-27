mod app;
mod domain;
mod error;
mod infra;
mod ui;

use clap::Parser;
use color_eyre::eyre::Result;

use app::action::Action;
use app::state::AppState;
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

    let mut state = AppState::new(project_name, args.profile);
    state.database_name = dsn.as_ref().and_then(|d| extract_database_name(d));

    let mut tui = TuiRunner::new()?.tick_rate(4.0).frame_rate(30.0);
    tui.enter()?;

    loop {
        if let Some(event) = tui.next_event().await {
            let action = handle_event(event, &state);

            match action {
                Action::Quit => state.should_quit = true,
                Action::Render => {
                    tui.terminal()
                        .draw(|frame| MainLayout::render(frame, &state))?;
                }
                Action::Resize(w, h) => {
                    tui.terminal()
                        .resize(ratatui::layout::Rect::new(0, 0, w, h))?;
                }
                Action::SwitchToBrowse => state.active_tab = 0,
                Action::SwitchToER => state.active_tab = 1,
                Action::ToggleFocus => state.focus_mode = !state.focus_mode,
                _ => {}
            }
        }

        if state.should_quit {
            break;
        }
    }

    tui.exit()?;
    Ok(())
}

fn extract_database_name(dsn: &str) -> Option<String> {
    dsn.rsplit('/').next().map(|s| s.to_string())
}
