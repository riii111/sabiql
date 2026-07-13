use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use sabiql_app::ports::outbound::DbOperationError;

fn epoch_days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn download_directory() -> PathBuf {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn download_export_path(file_name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86_400;
    let time_of_day = secs % 86_400;
    let (year, month, day) = epoch_days_to_ymd(days as i64);
    let timestamp = format!(
        "{year:04}{month:02}{day:02}_{:02}{:02}{:02}_{:03}",
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60,
        now.subsec_millis()
    );
    download_directory().join(format!("sabiql_export_{file_name}_{timestamp}.csv"))
}

fn temporary_export_path(final_path: &Path) -> PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let file_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export.csv");
    final_path.with_file_name(format!(
        ".{file_name}.{}.{}.part",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

struct TemporaryExportFile {
    path: PathBuf,
    cleanup: bool,
}

impl TemporaryExportFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            cleanup: true,
        }
    }

    fn disarm(&mut self) {
        self.cleanup = false;
    }
}

impl Drop for TemporaryExportFile {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct PublishedExportFile {
    path: PathBuf,
    cleanup: bool,
}

impl PublishedExportFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            cleanup: true,
        }
    }

    fn disarm(&mut self) {
        self.cleanup = false;
    }
}

impl Drop for PublishedExportFile {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub async fn export_to_downloads<F, Fut>(
    file_name: &str,
    write: F,
) -> Result<PathBuf, DbOperationError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<(), DbOperationError>>,
{
    export_to_path(download_export_path(file_name), write).await
}

pub async fn export_to_path<F, Fut>(
    final_path: PathBuf,
    write: F,
) -> Result<PathBuf, DbOperationError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<(), DbOperationError>>,
{
    export_to_path_with_cleanup(final_path, write, |path| std::fs::remove_file(path)).await
}

async fn export_to_path_with_cleanup<F, Fut, C>(
    final_path: PathBuf,
    write: F,
    cleanup: C,
) -> Result<PathBuf, DbOperationError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<(), DbOperationError>>,
    C: FnOnce(&Path) -> std::io::Result<()>,
{
    let temporary_path = temporary_export_path(&final_path);
    let mut temporary_file = TemporaryExportFile::new(temporary_path.clone());
    write(temporary_path.clone()).await?;

    tokio::fs::hard_link(&temporary_path, &final_path)
        .await
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let mut published_file = PublishedExportFile::new(final_path.clone());

    if cleanup(&temporary_path).is_ok() {
        temporary_file.disarm();
    }
    published_file.disarm();
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::Arc;

    use tempfile::tempdir;
    use tokio::sync::Barrier;
    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn cancellation_removes_partial_temporary_file() {
        let dir = tempdir().unwrap();
        let final_path = dir.path().join("export.csv");
        let (started_tx, started_rx) = oneshot::channel();
        let task = tokio::spawn(export_to_path(
            final_path.clone(),
            move |temporary_path| async move {
                tokio::fs::write(temporary_path, b"partial,csv\n")
                    .await
                    .unwrap();
                started_tx.send(()).ok();
                pending::<Result<(), DbOperationError>>().await
            },
        ));

        started_rx.await.unwrap();
        task.abort();
        task.await.unwrap_err();

        assert!(!final_path.exists());
        assert_eq!(dir.path().read_dir().unwrap().count(), 0);
    }

    #[tokio::test]
    async fn success_publishes_temporary_file_without_replacing_existing_file() {
        let dir = tempdir().unwrap();
        let final_path = dir.path().join("export.csv");

        let exported_path = export_to_path(final_path.clone(), |temporary_path| async move {
            tokio::fs::write(temporary_path, b"complete,csv\n")
                .await
                .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
        })
        .await
        .unwrap();

        assert_eq!(exported_path, final_path);
        assert_eq!(
            tokio::fs::read_to_string(&final_path).await.unwrap(),
            "complete,csv\n"
        );

        let error = export_to_path(final_path.clone(), |_| async { Ok(()) })
            .await
            .unwrap_err();
        assert!(matches!(error, DbOperationError::QueryFailed(_)));
        assert_eq!(
            tokio::fs::read_to_string(final_path).await.unwrap(),
            "complete,csv\n"
        );
    }

    #[tokio::test]
    async fn write_failure_leaves_no_partial_file() {
        let dir = tempdir().unwrap();
        let failed_write = dir.path().join("write.csv");
        let write_error = export_to_path(failed_write.clone(), |_| async {
            Err(DbOperationError::QueryFailed("write failed".to_string()))
        })
        .await
        .unwrap_err();
        assert!(matches!(write_error, DbOperationError::QueryFailed(_)));
        assert_eq!(dir.path().read_dir().unwrap().count(), 0);
    }

    #[tokio::test]
    async fn finalize_failure_leaves_no_partial_file() {
        let dir = tempdir().unwrap();
        let rename_dir = dir.path().join("rename");
        tokio::fs::create_dir(&rename_dir).await.unwrap();
        let failed_rename = rename_dir.join("export.csv");
        let rename_error = export_to_path(failed_rename, |temporary_path| async move {
            tokio::fs::write(&temporary_path, b"complete,csv\n")
                .await
                .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
            tokio::fs::remove_dir_all(temporary_path.parent().unwrap())
                .await
                .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
        })
        .await
        .unwrap_err();
        assert!(matches!(rename_error, DbOperationError::QueryFailed(_)));
        assert!(!rename_dir.exists());
    }

    #[tokio::test]
    async fn temporary_cleanup_failure_keeps_published_file_successful() {
        let dir = tempdir().unwrap();
        let final_path = dir.path().join("export.csv");

        let exported_path = export_to_path_with_cleanup(
            final_path.clone(),
            |temporary_path| async move {
                tokio::fs::write(temporary_path, b"complete,csv\n")
                    .await
                    .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
            },
            |_| Err(std::io::Error::other("cleanup failed")),
        )
        .await
        .unwrap();

        assert_eq!(exported_path, final_path);
        assert_eq!(
            tokio::fs::read_to_string(final_path).await.unwrap(),
            "complete,csv\n"
        );
    }

    #[tokio::test]
    async fn cancellation_after_publication_keeps_successful_file() {
        let dir = tempdir().unwrap();
        let final_path = dir.path().join("export.csv");
        let (published_tx, published_rx) = oneshot::channel();

        let task = tokio::spawn(export_to_path_with_cleanup(
            final_path.clone(),
            |temporary_path| async move {
                tokio::fs::write(temporary_path, b"complete,csv\n")
                    .await
                    .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
            },
            move |temporary_path| {
                published_tx.send(()).ok();
                std::fs::remove_file(temporary_path)
            },
        ));

        published_rx.await.unwrap();
        task.abort();
        let result = task.await.unwrap().unwrap();

        assert_eq!(result, final_path);
        assert_eq!(
            tokio::fs::read_to_string(final_path).await.unwrap(),
            "complete,csv\n"
        );
    }

    #[tokio::test]
    async fn concurrent_exports_with_same_final_path_allow_only_one_completion() {
        let dir = tempdir().unwrap();
        let final_path = dir.path().join("export.csv");
        let barrier = Arc::new(Barrier::new(2));

        let first = tokio::spawn(export_to_path(final_path.clone(), {
            let barrier = Arc::clone(&barrier);
            move |temporary_path| async move {
                tokio::fs::write(temporary_path, b"first\n")
                    .await
                    .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
                barrier.wait().await;
                Ok(())
            }
        }));
        let second = tokio::spawn(export_to_path(final_path.clone(), {
            let barrier = Arc::clone(&barrier);
            move |temporary_path| async move {
                tokio::fs::write(temporary_path, b"second\n")
                    .await
                    .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
                barrier.wait().await;
                Ok(())
            }
        }));

        let first = first.await.unwrap();
        let second = second.await.unwrap();

        assert!(first.is_ok() ^ second.is_ok());
        assert!(matches!(
            first,
            Ok(_) | Err(DbOperationError::QueryFailed(_))
        ));
        assert!(matches!(
            second,
            Ok(_) | Err(DbOperationError::QueryFailed(_))
        ));
        assert!(matches!(
            tokio::fs::read_to_string(final_path)
                .await
                .unwrap()
                .as_str(),
            "first\n" | "second\n"
        ));
        assert_eq!(dir.path().read_dir().unwrap().count(), 1);
    }
}
