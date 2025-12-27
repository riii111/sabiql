mod app;
mod domain;
mod error;
mod infra;
mod ui;

use clap::Parser;
use color_eyre::eyre::Result;

use app::action::Action;
use app::command::{command_to_action, parse_command};
use app::input_mode::InputMode;
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

                // Overlay actions
                Action::OpenTablePicker => {
                    state.show_table_picker = true;
                    state.filter_input.clear();
                    state.picker_selected = 0;
                }
                Action::CloseTablePicker => {
                    state.show_table_picker = false;
                }
                Action::OpenCommandPalette => {
                    state.show_command_palette = true;
                    state.picker_selected = 0;
                }
                Action::CloseCommandPalette => {
                    state.show_command_palette = false;
                }
                Action::OpenHelp => {
                    state.show_help = !state.show_help;
                }
                Action::CloseHelp => {
                    state.show_help = false;
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
                    // Handle follow-up action
                    if follow_up == Action::Quit {
                        state.should_quit = true;
                    } else if follow_up == Action::OpenHelp {
                        state.show_help = true;
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

                // Navigation
                Action::SelectNext => {
                    let max = if state.show_table_picker {
                        let filter_lower = state.filter_input.to_lowercase();
                        state
                            .tables
                            .iter()
                            .filter(|t| t.to_lowercase().contains(&filter_lower))
                            .count()
                            .saturating_sub(1)
                    } else {
                        10 // Placeholder max
                    };
                    if state.picker_selected < max {
                        state.picker_selected += 1;
                    }
                }
                Action::SelectPrevious => {
                    state.picker_selected = state.picker_selected.saturating_sub(1);
                }
                Action::SelectFirst => {
                    state.picker_selected = 0;
                }
                Action::SelectLast => {
                    let max = if state.show_table_picker {
                        let filter_lower = state.filter_input.to_lowercase();
                        state
                            .tables
                            .iter()
                            .filter(|t| t.to_lowercase().contains(&filter_lower))
                            .count()
                            .saturating_sub(1)
                    } else {
                        10
                    };
                    state.picker_selected = max;
                }

                // Selection
                Action::ConfirmSelection => {
                    if state.show_table_picker {
                        let filter_lower = state.filter_input.to_lowercase();
                        let filtered: Vec<&String> = state
                            .tables
                            .iter()
                            .filter(|t| t.to_lowercase().contains(&filter_lower))
                            .collect();
                        if let Some(table) = filtered.get(state.picker_selected) {
                            state.current_table = Some((*table).clone());
                        }
                        state.show_table_picker = false;
                    } else if state.show_command_palette {
                        state.show_command_palette = false;
                    }
                }

                // Escape
                Action::Escape => {
                    if state.show_table_picker {
                        state.show_table_picker = false;
                    } else if state.show_command_palette {
                        state.show_command_palette = false;
                    } else if state.show_help {
                        state.show_help = false;
                    }
                }

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
