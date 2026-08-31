use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use sabiql_app::ports::outbound::DbOperationError;
use tokio::io::{AsyncWriteExt, BufWriter};

pub(super) const CSV_FLUSH_THRESHOLD: usize = 64 * 1024;

fn new_csv_writer() -> csv::Writer<Vec<u8>> {
    csv::WriterBuilder::new().from_writer(Vec::with_capacity(CSV_FLUSH_THRESHOLD))
}

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

pub(super) fn download_export_path(file_name: &str) -> PathBuf {
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

struct RemoveOnDropGuard {
    path: Option<PathBuf>,
}

impl RemoveOnDropGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for RemoveOnDropGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(super) enum CsvOutputError {
    File(std::io::Error),
    Process(std::io::Error),
}

impl From<std::io::Error> for CsvOutputError {
    fn from(error: std::io::Error) -> Self {
        Self::Process(error)
    }
}

impl CsvOutputError {
    pub(super) fn into_db_operation_error(self) -> DbOperationError {
        match self {
            Self::File(error) => DbOperationError::ExportIo(Arc::new(error)),
            Self::Process(error) => DbOperationError::QueryFailed(error.to_string()),
        }
    }
}

pub(super) async fn export_to_downloads<F, Fut>(
    file_name: &str,
    write: F,
) -> Result<PathBuf, DbOperationError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<(), DbOperationError>>,
{
    export_to_path(download_export_path(file_name), write).await
}

pub(super) async fn export_to_path<F, Fut>(
    final_path: PathBuf,
    write: F,
) -> Result<PathBuf, DbOperationError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<(), DbOperationError>>,
{
    export_to_path_with_cleanup(final_path, write, |path| std::fs::remove_file(path)).await
}

pub(super) struct CsvFileWriter {
    file: BufWriter<tokio::fs::File>,
    csv_writer: csv::Writer<Vec<u8>>,
}

impl CsvFileWriter {
    pub(super) async fn create(path: PathBuf) -> Result<Self, DbOperationError> {
        let file = tokio::fs::File::create(path)
            .await
            .map_err(|error| DbOperationError::ExportIo(Arc::new(error)))?;
        Ok(Self {
            file: BufWriter::new(file),
            csv_writer: new_csv_writer(),
        })
    }

    pub(super) async fn write_record<I>(&mut self, record: I) -> Result<(), DbOperationError>
    where
        I: IntoIterator,
        I::Item: AsRef<[u8]>,
    {
        self.csv_writer.write_record(record)?;
        self.csv_writer
            .flush()
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        if self.csv_writer.get_ref().len() >= CSV_FLUSH_THRESHOLD {
            self.flush_buffer().await?;
        }
        Ok(())
    }

    pub(super) async fn finish(mut self) -> Result<(), DbOperationError> {
        let encoded = self
            .csv_writer
            .into_inner()
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        self.file
            .write_all(&encoded)
            .await
            .map_err(|error| DbOperationError::ExportIo(Arc::new(error)))?;
        self.file
            .flush()
            .await
            .map_err(|error| DbOperationError::ExportIo(Arc::new(error)))
    }

    async fn flush_buffer(&mut self) -> Result<(), DbOperationError> {
        let csv_writer = std::mem::replace(&mut self.csv_writer, new_csv_writer());
        let encoded = csv_writer
            .into_inner()
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        self.file
            .write_all(&encoded)
            .await
            .map_err(|error| DbOperationError::ExportIo(Arc::new(error)))?;
        self.file
            .flush()
            .await
            .map_err(|error| DbOperationError::ExportIo(Arc::new(error)))?;
        Ok(())
    }
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
    let mut temporary_file = RemoveOnDropGuard::new(temporary_path.clone());
    write(temporary_path.clone()).await?;

    tokio::fs::hard_link(&temporary_path, &final_path)
        .await
        .map_err(|error| DbOperationError::ExportIo(Arc::new(error)))?;
    let mut published_file = RemoveOnDropGuard::new(final_path.clone());

    if cleanup(&temporary_path).is_ok() {
        temporary_file.disarm();
    }
    published_file.disarm();
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use std::future::pending;

    use tempfile::tempdir;
    use tokio::sync::Barrier;
    use tokio::sync::oneshot;

    use super::*;

    const MEMORY_MEASUREMENT_FIELD_BYTES: usize = 8 * 1024 * 1024;

    fn assert_export_io_source(error: &DbOperationError) {
        assert!(matches!(error, DbOperationError::ExportIo(_)));
        let source = std::error::Error::source(error).expect("ExportIo source");
        assert!(source.downcast_ref::<Arc<std::io::Error>>().is_some());
    }

    #[cfg(unix)]
    async fn read_only_csv_writer(path: &Path) -> CsvFileWriter {
        tokio::fs::write(path, []).await.unwrap();
        let file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(path)
            .await
            .unwrap();
        CsvFileWriter {
            file: BufWriter::new(file),
            csv_writer: new_csv_writer(),
        }
    }

    #[test]
    fn csv_output_errors_keep_file_and_process_failures_distinct() {
        let file_error =
            CsvOutputError::File(std::io::Error::other("disk full")).into_db_operation_error();
        assert_export_io_source(&file_error);

        let process_error =
            CsvOutputError::Process(std::io::Error::other("pipe closed")).into_db_operation_error();
        assert!(matches!(
            &process_error,
            DbOperationError::QueryFailed(details) if details == "pipe closed"
        ));
        assert!(std::error::Error::source(&process_error).is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn read_only_file_flush_failure_is_export_io() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("read-only.csv");
        let mut writer = read_only_csv_writer(&path).await;
        writer.csv_writer.write_record(["row"]).unwrap();
        writer.csv_writer.flush().unwrap();

        let error = writer.finish().await.unwrap_err();
        assert_export_io_source(&error);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn read_only_file_write_failure_is_export_io() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("read-only.csv");
        let mut writer = read_only_csv_writer(&path).await;
        let value = "x".repeat(CSV_FLUSH_THRESHOLD);

        let error = writer.write_record([value.as_str()]).await.unwrap_err();
        assert_export_io_source(&error);
    }

    #[tokio::test]
    async fn writes_large_record_without_changing_csv_bytes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("large-record.csv");
        let value = "x".repeat(MEMORY_MEASUREMENT_FIELD_BYTES);
        let mut writer = CsvFileWriter::create(path.clone()).await.unwrap();

        writer.write_record([value.as_str()]).await.unwrap();
        writer.finish().await.unwrap();

        assert_eq!(
            tokio::fs::metadata(path).await.unwrap().len(),
            (MEMORY_MEASUREMENT_FIELD_BYTES + 1) as u64
        );
    }

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
        assert!(matches!(error, DbOperationError::ExportIo(_)));
        assert!(std::error::Error::source(&error).is_some());
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
        assert!(matches!(rename_error, DbOperationError::ExportIo(_)));
        assert!(std::error::Error::source(&rename_error).is_some());
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
        assert!(matches!(first, Ok(_) | Err(DbOperationError::ExportIo(_))));
        assert!(matches!(second, Ok(_) | Err(DbOperationError::ExportIo(_))));
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
