use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use tokio::task::JoinHandle;

struct TaskEntry {
    released: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct MetadataTaskRegistry {
    active: Mutex<HashMap<u64, TaskEntry>>,
    next_id: AtomicU64,
}

impl MetadataTaskRegistry {
    pub fn spawn<F>(registry: &Arc<Self>, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task_id = registry.next_id.fetch_add(1, Ordering::Relaxed);
        let released = Arc::new(AtomicBool::new(false));
        registry
            .active
            .lock()
            .expect("metadata task registry lock poisoned")
            .insert(
                task_id,
                TaskEntry {
                    released: Arc::clone(&released),
                    handle: None,
                },
            );

        let registration_guard = TaskRegistrationGuard {
            registry: Arc::downgrade(registry),
            task_id,
            released: Arc::clone(&released),
        };
        let handle = tokio::spawn(async move {
            let _registration_guard = registration_guard;
            task.await;
        });

        let mut handle = Some(handle);
        let abort = {
            let mut active = registry
                .active
                .lock()
                .expect("metadata task registry lock poisoned");
            match active.get_mut(&task_id) {
                Some(entry) if !released.load(Ordering::SeqCst) => {
                    entry.handle = handle.take();
                    false
                }
                Some(_) | None => {
                    active.remove(&task_id);
                    true
                }
            }
        };

        if abort {
            handle
                .take()
                .expect("metadata task handle should be available")
                .abort();
        }
    }

    pub async fn cancel(&self) {
        for handle in self.abort() {
            let _ = handle.await;
        }
    }

    pub fn abort(&self) -> Vec<JoinHandle<()>> {
        let handles = self
            .active
            .lock()
            .expect("metadata task registry lock poisoned")
            .drain()
            .filter_map(|(_, entry)| entry.handle)
            .collect::<Vec<_>>();

        for handle in &handles {
            handle.abort();
        }
        handles
    }

    fn remove_released(&self, task_id: u64, released: &Arc<AtomicBool>) {
        let mut active = self
            .active
            .lock()
            .expect("metadata task registry lock poisoned");
        let should_remove = active
            .get(&task_id)
            .is_some_and(|entry| Arc::ptr_eq(&entry.released, released));
        if should_remove {
            active.remove(&task_id);
        }
    }
}

impl Drop for MetadataTaskRegistry {
    fn drop(&mut self) {
        let active = self
            .active
            .get_mut()
            .expect("metadata task registry lock poisoned");
        for entry in active.values_mut() {
            if let Some(handle) = entry.handle.take() {
                handle.abort();
            }
        }
    }
}

struct TaskRegistrationGuard {
    registry: Weak<MetadataTaskRegistry>,
    task_id: u64,
    released: Arc<AtomicBool>,
}

impl Drop for TaskRegistrationGuard {
    fn drop(&mut self) {
        self.released.store(true, Ordering::SeqCst);
        if let Some(registry) = self.registry.upgrade() {
            registry.remove_released(self.task_id, &self.released);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::atomic::AtomicUsize;
    use tokio::sync::oneshot;
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
        assert_eq!(registry.active.lock().unwrap().len(), 4);

        registry.cancel().await;

        timeout(Duration::from_secs(1), async {
            while dropped.load(Ordering::SeqCst) != 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("all prefetch tasks should be dropped");
        assert!(registry.active.lock().unwrap().is_empty());
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
            while !registry.active.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("released task should be removed");
    }
}
