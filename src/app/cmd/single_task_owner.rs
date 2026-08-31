use std::sync::Mutex;

use tokio::task::JoinHandle;

#[derive(Default)]
pub struct SingleTaskOwner {
    active: Mutex<Option<JoinHandle<()>>>,
}

impl SingleTaskOwner {
    pub async fn replace<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Some(task) = self.abort() {
            let _ = task.await;
        }
        let task = tokio::spawn(task);
        *self.active.lock().expect("single task owner lock poisoned") = Some(task);
    }

    pub fn abort(&self) -> Option<JoinHandle<()>> {
        let task = self
            .active
            .lock()
            .expect("single task owner lock poisoned")
            .take();
        if let Some(task) = &task {
            task.abort();
        }
        task
    }

    pub async fn cancel(&self) {
        if let Some(task) = self.abort() {
            let _ = task.await;
        }
    }
}

impl Drop for SingleTaskOwner {
    fn drop(&mut self) {
        if let Some(task) = self
            .active
            .get_mut()
            .expect("single task owner lock poisoned")
            .take()
        {
            task.abort();
        }
    }
}

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
    async fn cancel_drops_active_task() {
        let owner = SingleTaskOwner::default();
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = oneshot::channel();
        let guard = DropSignal(Arc::clone(&dropped));

        owner
            .replace(async move {
                let _guard = guard;
                started_tx.send(()).ok();
                std::future::pending::<()>().await;
            })
            .await;

        started_rx.await.expect("query task should start");
        owner.cancel().await;

        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn replacement_waits_for_previous_task_to_drop() {
        let owner = SingleTaskOwner::default();
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = oneshot::channel();
        let first_guard = DropSignal(Arc::clone(&dropped));

        owner
            .replace(async move {
                let _guard = first_guard;
                started_tx.send(()).ok();
                std::future::pending::<()>().await;
            })
            .await;

        started_rx.await.expect("query task should start");

        let (replacement_started_tx, replacement_started_rx) = oneshot::channel();
        owner
            .replace(async move {
                assert!(dropped.load(Ordering::SeqCst));
                replacement_started_tx.send(()).ok();
            })
            .await;

        replacement_started_rx
            .await
            .expect("replacement task should start");
        owner.cancel().await;
    }
}
