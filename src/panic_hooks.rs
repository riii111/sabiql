use std::io::stdout;
use std::panic;

use color_eyre::eyre::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event::{DisableBracketedPaste, DisableMouseCapture},
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};

#[allow(
    clippy::print_stderr,
    reason = "panic hook must write to stderr after terminal restore"
)]
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
    if !crossterm::terminal::is_raw_mode_enabled()? {
        return Ok(());
    }

    restore_terminal_steps(|step| match step {
        RestoreStep::Cursor => {
            execute!(stdout(), SetCursorStyle::DefaultUserShape).map_err(Into::into)
        }
        RestoreStep::AlternateScreen => {
            execute!(stdout(), LeaveAlternateScreen).map_err(Into::into)
        }
        RestoreStep::MouseCapture => execute!(stdout(), DisableMouseCapture).map_err(Into::into),
        RestoreStep::BracketedPaste => {
            execute!(stdout(), DisableBracketedPaste).map_err(Into::into)
        }
        RestoreStep::Raw => disable_raw_mode().map_err(Into::into),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoreStep {
    Cursor,
    BracketedPaste,
    MouseCapture,
    AlternateScreen,
    Raw,
}

fn restore_terminal_steps<F>(mut restore: F) -> Result<()>
where
    F: FnMut(RestoreStep) -> Result<()>,
{
    let mut first_error = None;
    for step in [
        RestoreStep::Cursor,
        RestoreStep::BracketedPaste,
        RestoreStep::MouseCapture,
        RestoreStep::AlternateScreen,
        RestoreStep::Raw,
    ] {
        if let Err(error) = restore(step)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continues_restore_steps_after_a_failure() {
        let mut restored = Vec::new();

        let error = restore_terminal_steps(|step| {
            restored.push(step);
            if step == RestoreStep::BracketedPaste {
                Err(color_eyre::eyre::eyre!("bracketed paste restore failed"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "bracketed paste restore failed");
        assert_eq!(
            restored,
            vec![
                RestoreStep::Cursor,
                RestoreStep::BracketedPaste,
                RestoreStep::MouseCapture,
                RestoreStep::AlternateScreen,
                RestoreStep::Raw,
            ]
        );
    }
}
