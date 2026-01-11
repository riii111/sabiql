use std::io::{Stdout, stdout};
use std::time::Duration;

use color_eyre::eyre::Result;
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
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::event::Event;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub struct TuiRunner {
    terminal: Tui,
    event_rx: UnboundedReceiver<Event>,
    event_tx: UnboundedSender<Event>,
    task: Option<JoinHandle<()>>,
    cancellation_token: CancellationToken,
    tick_rate: f64,
    frame_rate: f64,
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
            tick_rate: 4.0,
            frame_rate: 30.0,
        })
    }

    pub fn tick_rate(mut self, rate: f64) -> Self {
        self.tick_rate = rate;
        self
    }

    pub fn frame_rate(mut self, rate: f64) -> Self {
        self.frame_rate = rate;
        self
    }

    pub fn enter(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(
            stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        )?;
        self.start_event_loop();
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        self.stop_event_loop();
        if crossterm::terminal::is_raw_mode_enabled()? {
            execute!(
                stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                DisableBracketedPaste
            )?;
            disable_raw_mode()?;
        }
        Ok(())
    }

    fn start_event_loop(&mut self) {
        let tick_rate = self.tick_rate;
        let frame_rate = self.frame_rate;
        let event_tx = self.event_tx.clone();
        let cancellation_token = self.cancellation_token.clone();

        self.task = Some(tokio::spawn(async move {
            let mut event_stream = EventStream::new();
            let mut tick_interval = tokio::time::interval(Duration::from_secs_f64(1.0 / tick_rate));
            let mut render_interval =
                tokio::time::interval(Duration::from_secs_f64(1.0 / frame_rate));

            let _ = event_tx.send(Event::Init);

            loop {
                let event = tokio::select! {
                    _ = cancellation_token.cancelled() => break,
                    _ = tick_interval.tick() => Event::Tick,
                    _ = render_interval.tick() => Event::Render,
                    crossterm_event = event_stream.next().fuse() => {
                        match crossterm_event {
                            Some(Ok(evt)) => match evt {
                                CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat => {
                                    Event::Key(key)
                                }
                                CrosstermEvent::Resize(x, y) => Event::Resize(x, y),
                                _ => continue,
                            },
                            Some(Err(_)) => break,
                            None => break,
                        }
                    }
                };

                if event_tx.send(event).is_err() {
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

    pub async fn next_event(&mut self) -> Option<Event> {
        self.event_rx.recv().await
    }

    pub fn terminal(&mut self) -> &mut Tui {
        &mut self.terminal
    }
}
