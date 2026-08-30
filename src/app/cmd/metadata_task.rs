use std::pin::Pin;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

type MetadataTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

enum Command {
    Spawn(MetadataTask),
    Abort(oneshot::Sender<()>),
    #[cfg(test)]
    Count(oneshot::Sender<usize>),
}

#[derive(Default)]
struct RegistryState {
    command_tx: Option<mpsc::UnboundedSender<Command>>,
    owner: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct MetadataTaskRegistry {
    state: Mutex<RegistryState>,
}

impl MetadataTaskRegistry {
    pub fn spawn<F>(registry: &Arc<Self>, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let command_tx = Self::command_tx(registry);
        command_tx.send(Command::Spawn(Box::pin(task))).ok();
    }

    pub async fn cancel(&self) {
        if let Some(handle) = self.abort() {
            let _ = handle.await;
        }
    }

    pub fn abort(&self) -> Option<JoinHandle<()>> {
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

    #[cfg(test)]
    async fn active_count(&self) -> usize {
        let command_tx = self
            .state
            .lock()
            .expect("metadata task registry lock poisoned")
            .command_tx
            .clone()
            .expect("metadata task registry owner should be running");
        let (count_tx, count_rx) = oneshot::channel();
        command_tx
            .send(Command::Count(count_tx))
            .expect("metadata task registry owner should be running");
        count_rx
            .await
            .expect("metadata task registry owner should report task count")
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
                #[cfg(test)]
                Some(Command::Count(count)) => {
                    let _ = count.send(tasks.len());
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
        assert_eq!(registry.active_count().await, 4);

        registry.cancel().await;

        timeout(Duration::from_secs(1), async {
            while dropped.load(Ordering::SeqCst) != 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("all prefetch tasks should be dropped");
        assert_eq!(registry.active_count().await, 0);
    }

    #[tokio::test]
    async fn removes_released_task_from_registry() {
        let registry = Arc::new(MetadataTaskRegistry::default());
        let (done_tx, done_rx) = oneshot::channel();
        MetadataTaskRegistry::spawn(&registry, async move {
            done_tx.send(()).ok();
        });

        done_rx.await.expect("task should complete");
        timeout(Duration::from_secs(1), async {
            while registry.active_count().await != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("released task should be removed");
    }
}
