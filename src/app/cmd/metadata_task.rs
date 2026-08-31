use std::pin::Pin;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

type MetadataTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

enum Command {
    Spawn(MetadataTask),
    Abort(oneshot::Sender<()>),
}

#[derive(Default)]
struct RegistryState {
    command_tx: Option<mpsc::UnboundedSender<Command>>,
    owner: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub(crate) struct MetadataTaskRegistry {
    state: Mutex<RegistryState>,
}

impl MetadataTaskRegistry {
    pub(crate) fn spawn<F>(registry: &Arc<Self>, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let command_tx = Self::command_tx(registry);
        command_tx.send(Command::Spawn(Box::pin(task))).ok();
    }

    pub(crate) async fn cancel(&self) {
        if let Some(handle) = self.abort() {
            let _ = handle.await;
        }
    }

    pub(crate) fn abort(&self) -> Option<JoinHandle<()>> {
        let command_tx = self
            .state
            .lock()
            .expect("metadata task registry lock poisoned")
            .command_tx
            .clone()?;
        let (done_tx, done_rx) = oneshot::channel();
        command_tx.send(Command::Abort(done_tx)).ok()?;
        Some(tokio::spawn(async move {
            let _ = done_rx.await;
        }))
    }

    fn command_tx(registry: &Arc<Self>) -> mpsc::UnboundedSender<Command> {
        let mut state = registry
            .state
            .lock()
            .expect("metadata task registry lock poisoned");
        if let Some(command_tx) = &state.command_tx {
            return command_tx.clone();
        }

        let (command_tx, commands) = mpsc::unbounded_channel();
        let owner = tokio::spawn(run(commands));
        state.command_tx = Some(command_tx.clone());
        state.owner = Some(owner);
        command_tx
    }
}

async fn run(mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Spawn(task)) => {
                    tasks.spawn(task);
                }
                Some(Command::Abort(done)) => {
                    tasks.shutdown().await;
                    let _ = done.send(());
                }
                None => {
                    tasks.shutdown().await;
                    return;
                }
            },
            result = tasks.join_next(), if !tasks.is_empty() => {
                let _ = result;
            }
        }
    }
}

impl Drop for MetadataTaskRegistry {
    fn drop(&mut self) {
        let state = self
            .state
            .get_mut()
            .expect("metadata task registry lock poisoned");
        if let Some(owner) = state.owner.take() {
            owner.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{Duration, timeout};

    use super::*;

    struct DropSignal(Arc<AtomicUsize>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn tracks_four_concurrent_prefetch_tasks_and_cleans_them_up() {
        let registry = Arc::new(MetadataTaskRegistry::default());
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut started = Vec::new();

        for _ in 0..4 {
            let (started_tx, started_rx) = oneshot::channel();
            started.push(started_rx);
            let guard = DropSignal(Arc::clone(&dropped));
            MetadataTaskRegistry::spawn(&registry, async move {
                let _guard = guard;
                started_tx.send(()).ok();
                pending::<()>().await;
            });
        }

        for signal in started {
            signal.await.expect("prefetch task should start");
        }
        registry.cancel().await;

        timeout(Duration::from_secs(1), async {
            while dropped.load(Ordering::SeqCst) != 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("all prefetch tasks should be dropped");
    }

    #[tokio::test]
    async fn reaps_completed_task_from_registry() {
        let registry = Arc::new(MetadataTaskRegistry::default());
        let reaped = Arc::new(AtomicUsize::new(0));
        let reaped_signal = DropSignal(Arc::clone(&reaped));
        MetadataTaskRegistry::spawn(&registry, async move {
            std::panic::panic_any(reaped_signal);
        });

        timeout(Duration::from_secs(1), async {
            while reaped.load(Ordering::SeqCst) != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("completed task should be reaped");
    }
}
