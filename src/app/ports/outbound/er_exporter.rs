use std::path::{Path, PathBuf};

use crate::domain::ErTableInfo;

#[derive(Debug, thiserror::Error)]
pub enum ErExportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Export(String),
}

pub type ErExportResult<T> = Result<T, ErExportError>;

pub trait ErDiagramExporter: Send + Sync {
    fn generate_and_export(
        &self,
        tables: &[ErTableInfo],
        filename: &str,
        cache_dir: &Path,
        browser: Option<&str>,
    ) -> ErExportResult<PathBuf>;
}
