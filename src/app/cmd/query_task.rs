use std::future::Future;
use std::sync::Mutex;

use tokio::task::AbortHandle;

#[derive(Default)]
pub struct QueryTaskRegistry {
    active: Mutex<Option<AbortHandle>>,
}

impl QueryTaskRegistry {
    pub fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.cancel();
        let handle = tokio::spawn(task);
        *self
            .active
            .lock()
            .expect("query task registry lock poisoned") = Some(handle.abort_handle());
    }

    pub fn cancel(&self) {
        let handle = self
            .active
            .lock()
            .expect("query task registry lock poisoned")
            .take();
        if let Some(handle) = handle {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::oneshot;
    use tokio::time::{Duration, timeout};

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

        registry.spawn(async move {
            let _guard = guard;
            started_tx.send(()).ok();
            std::future::pending::<()>().await;
        });

        started_rx.await.expect("query task should start");
        registry.cancel();

        timeout(Duration::from_secs(1), async {
            while !dropped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled query task should be dropped");
    }
}
