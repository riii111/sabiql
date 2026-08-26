use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Default)]
pub struct TaskRegistry {
    active: Mutex<Option<JoinHandle<()>>>,
}

impl TaskRegistry {
    /// Starts a task after cancelling any currently active task.
    ///
    /// Each registry instance intentionally permits only one active task at a
    /// time; separate instances are used for queries and table details.
    pub async fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut active = self.active.lock().await;
        if let Some(handle) = active.take() {
            handle.abort();
            let _ = handle.await;
        }
        let handle = tokio::spawn(task);
        *active = Some(handle);
    }

    pub async fn cancel(&self) {
        let handle = self.active.lock().await.take();
        if let Some(handle) = handle {
            handle.abort();
            let _ = handle.await;
        }
    }
}

pub type QueryTaskRegistry = TaskRegistry;
pub type TableDetailTaskRegistry = TaskRegistry;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::oneshot;

    use super::*;

    struct DropSignal(Arc<AtomicBool>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn cancel_drops_active_query_task() {
        let registry = QueryTaskRegistry::default();
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = oneshot::channel();
        let guard = DropSignal(Arc::clone(&dropped));

        registry
            .spawn(async move {
                let _guard = guard;
                started_tx.send(()).ok();
                std::future::pending::<()>().await;
            })
            .await;

        started_rx.await.expect("query task should start");
        registry.cancel().await;

        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn replacement_waits_for_previous_query_task_to_drop() {
        let registry = QueryTaskRegistry::default();
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = oneshot::channel();
        let first_guard = DropSignal(Arc::clone(&dropped));

        registry
            .spawn(async move {
                let _guard = first_guard;
                started_tx.send(()).ok();
                std::future::pending::<()>().await;
            })
            .await;

        started_rx.await.expect("query task should start");

        let (replacement_started_tx, replacement_started_rx) = oneshot::channel();
        registry
            .spawn(async move {
                assert!(dropped.load(Ordering::SeqCst));
                replacement_started_tx.send(()).ok();
            })
            .await;

        replacement_started_rx
            .await
            .expect("replacement query task should start");
        registry.cancel().await;
    }
}
