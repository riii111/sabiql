use std::io::{self, Stdout, stdout};

use color_eyre::eyre::{Result, eyre};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event as CrosstermEvent, EventStream, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::{FutureExt, StreamExt};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::app::ports::inbound::InputEvent;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;
type TerminalEvent = Result<InputEvent, io::Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalMode {
    Raw,
    AlternateScreen,
    MouseCapture,
    BracketedPaste,
}

#[derive(Default)]
struct TerminalState {
    enabled_modes: u8,
}

impl TerminalState {
    fn enable(&mut self, mode: TerminalMode) {
        self.enabled_modes |= mode.mask();
    }

    fn is_enabled(&self, mode: TerminalMode) -> bool {
        self.enabled_modes & mode.mask() != 0
    }

    fn disable(&mut self, mode: TerminalMode) {
        self.enabled_modes &= !mode.mask();
    }

    fn any_enabled(&self) -> bool {
        self.enabled_modes != 0
    }
}

impl TerminalMode {
    fn mask(self) -> u8 {
        match self {
            Self::Raw => 1 << 0,
            Self::AlternateScreen => 1 << 1,
            Self::MouseCapture => 1 << 2,
            Self::BracketedPaste => 1 << 3,
        }
    }
}

pub struct TuiRunner {
    terminal: Tui,
    event_rx: UnboundedReceiver<TerminalEvent>,
    event_tx: UnboundedSender<TerminalEvent>,
    task: Option<JoinHandle<()>>,
    cancellation_token: CancellationToken,
    terminal_state: TerminalState,
}

impl TuiRunner {
    pub fn new() -> Result<Self> {
        let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let cancellation_token = CancellationToken::new();

        Ok(Self {
            terminal,
            event_rx,
            event_tx,
            task: None,
            cancellation_token,
            terminal_state: TerminalState::default(),
        })
    }

    pub fn enter(&mut self) -> Result<()> {
        enter_terminal_modes(&mut self.terminal_state, |mode| match mode {
            TerminalMode::Raw => enable_raw_mode().map_err(Into::into),
            TerminalMode::AlternateScreen => {
                execute!(stdout(), EnterAlternateScreen).map_err(Into::into)
            }
            TerminalMode::MouseCapture => {
                execute!(stdout(), EnableMouseCapture).map_err(Into::into)
            }
            TerminalMode::BracketedPaste => {
                execute!(stdout(), EnableBracketedPaste).map_err(Into::into)
            }
        })?;
        self.start_event_loop();
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        self.stop_event_loop();
        self.restore_terminal()
    }

    fn start_event_loop(&mut self) {
        let event_tx = self.event_tx.clone();
        let cancellation_token = self.cancellation_token.clone();

        self.task = Some(tokio::spawn(async move {
            let mut event_stream = EventStream::new();

            let _ = event_tx.send(Ok(InputEvent::Init));

            loop {
                let crossterm_event = tokio::select! {
                    () = cancellation_token.cancelled() => break,
                    crossterm_event = event_stream.next().fuse() => crossterm_event,
                };

                let Some(crossterm_event) = crossterm_event else {
                    let _ = event_tx.send(Err(event_stream_ended()));
                    break;
                };
                let crossterm_event = match crossterm_event {
                    Ok(event) => event,
                    Err(error) => {
                        let _ = event_tx.send(Err(error));
                        break;
                    }
                };
                let event = match crossterm_event {
                    CrosstermEvent::Key(key)
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
                    {
                        InputEvent::Key(super::event::key_translator::translate(key))
                    }
                    CrosstermEvent::Resize(x, y) => InputEvent::Resize(x, y),
                    CrosstermEvent::Paste(text) => InputEvent::Paste(text),
                    _ => continue,
                };

                if event_tx.send(Ok(event)).is_err() {
                    break;
                }
            }
        }));
    }

    fn stop_event_loop(&mut self) {
        self.cancellation_token.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }

    pub async fn next_event(&mut self) -> Result<InputEvent> {
        receive_event(self.event_rx.recv().await)
    }

    pub fn try_next_event(&mut self) -> Result<Option<InputEvent>> {
        match self.event_rx.try_recv() {
            Ok(event) => receive_event(Some(event)).map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(eyre!("terminal event channel closed unexpectedly"))
            }
        }
    }

    pub fn terminal(&mut self) -> &mut Tui {
        &mut self.terminal
    }

    fn restore_terminal(&mut self) -> Result<()> {
        let mut first_error = if self.terminal_state.any_enabled()
            && let Err(error) = execute!(stdout(), SetCursorStyle::DefaultUserShape)
        {
            Some(error.into())
        } else {
            None
        };

        if let Err(error) = restore_terminal_modes(&mut self.terminal_state, |mode| match mode {
            TerminalMode::Raw => disable_raw_mode().map_err(Into::into),
            TerminalMode::AlternateScreen => {
                execute!(stdout(), LeaveAlternateScreen).map_err(Into::into)
            }
            TerminalMode::MouseCapture => {
                execute!(stdout(), DisableMouseCapture).map_err(Into::into)
            }
            TerminalMode::BracketedPaste => {
                execute!(stdout(), DisableBracketedPaste).map_err(Into::into)
            }
        }) && first_error.is_none()
        {
            first_error = Some(error);
        }

        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for TuiRunner {
    fn drop(&mut self) {
        self.stop_event_loop();
        let _ = self.restore_terminal();
    }
}

fn restore_terminal_modes<F>(state: &mut TerminalState, mut restore: F) -> Result<()>
where
    F: FnMut(TerminalMode) -> Result<()>,
{
    let mut first_error = None;
    for mode in [
        TerminalMode::BracketedPaste,
        TerminalMode::MouseCapture,
        TerminalMode::AlternateScreen,
        TerminalMode::Raw,
    ] {
        if !state.is_enabled(mode) {
            continue;
        }

        match restore(mode) {
            Ok(()) => state.disable(mode),
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn enter_terminal_modes<F>(state: &mut TerminalState, mut enter: F) -> Result<()>
where
    F: FnMut(TerminalMode) -> Result<()>,
{
    for mode in [
        TerminalMode::Raw,
        TerminalMode::AlternateScreen,
        TerminalMode::MouseCapture,
        TerminalMode::BracketedPaste,
    ] {
        state.enable(mode);
        enter(mode)?;
    }
    Ok(())
}

fn receive_event(event: Option<TerminalEvent>) -> Result<InputEvent> {
    match event {
        Some(Ok(event)) => Ok(event),
        Some(Err(error)) => Err(error.into()),
        None => Err(eyre!("terminal event channel closed unexpectedly")),
    }
}

fn event_stream_ended() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "terminal event stream ended")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    #[test]
    fn restores_enabled_modes_in_reverse_order() {
        let mut state = TerminalState {
            enabled_modes: 0b1111,
        };
        let mut restored = Vec::new();

        restore_terminal_modes(&mut state, |mode| {
            restored.push(mode);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            restored,
            vec![
                TerminalMode::BracketedPaste,
                TerminalMode::MouseCapture,
                TerminalMode::AlternateScreen,
                TerminalMode::Raw,
            ]
        );
        assert!(!state.any_enabled());
    }

    #[test]
    fn continues_restoring_after_a_mode_failure() {
        let mut state = TerminalState {
            enabled_modes: 0b1111,
        };
        let mut restored = VecDeque::new();

        let error = restore_terminal_modes(&mut state, |mode| {
            restored.push_back(mode);
            if mode == TerminalMode::MouseCapture {
                Err(eyre!("mouse restore failed"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "mouse restore failed");
        assert_eq!(
            restored.into_iter().collect::<Vec<_>>(),
            vec![
                TerminalMode::BracketedPaste,
                TerminalMode::MouseCapture,
                TerminalMode::AlternateScreen,
                TerminalMode::Raw,
            ]
        );
        assert!(!state.is_enabled(TerminalMode::BracketedPaste));
        assert!(state.is_enabled(TerminalMode::MouseCapture));
        assert!(!state.is_enabled(TerminalMode::AlternateScreen));
        assert!(!state.is_enabled(TerminalMode::Raw));
    }

    #[test]
    fn failed_enter_stage_remains_owned_for_cleanup() {
        for failed_mode in [
            TerminalMode::Raw,
            TerminalMode::AlternateScreen,
            TerminalMode::MouseCapture,
            TerminalMode::BracketedPaste,
        ] {
            let mut state = TerminalState::default();
            let mut entered = Vec::new();

            let error = enter_terminal_modes(&mut state, |mode| {
                entered.push(mode);
                if mode == failed_mode {
                    Err(eyre!("terminal enter failed"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();

            assert_eq!(error.to_string(), "terminal enter failed");
            assert_eq!(entered.last(), Some(&failed_mode));
            assert!(state.is_enabled(failed_mode));
        }
    }

    #[test]
    fn receive_event_returns_event_stream_errors() {
        let error = receive_event(Some(Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "event read failed",
        ))))
        .unwrap_err();

        assert_eq!(error.to_string(), "event read failed");
    }

    #[test]
    fn receive_event_returns_event_stream_eof_as_error() {
        let error = receive_event(Some(Err(event_stream_ended()))).unwrap_err();

        assert_eq!(error.to_string(), "terminal event stream ended");
    }
}
