use std::io::stdout;
use std::panic;

use color_eyre::eyre::Result;
use crossterm::{
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};

pub fn install_hooks() -> Result<()> {
    let hook_builder = color_eyre::config::HookBuilder::default().display_env_section(false);
    let (panic_hook, eyre_hook) = hook_builder.into_hooks();
    eyre_hook.install()?;

    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        eprintln!("{}", panic_hook.panic_report(panic_info));
    }));

    Ok(())
}

fn restore_terminal() -> Result<()> {
    if crossterm::terminal::is_raw_mode_enabled()? {
        execute!(stdout(), LeaveAlternateScreen)?;
        disable_raw_mode()?;
    }
    Ok(())
}
