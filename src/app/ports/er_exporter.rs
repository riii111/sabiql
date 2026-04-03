use std::path::{Path, PathBuf};

use super::{GraphvizError, ViewerError};
use crate::domain::ErTableInfo;

#[derive(Debug, thiserror::Error)]
pub enum ErExportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Graphviz(#[from] GraphvizError),
    #[error("{0}")]
    Viewer(#[from] ViewerError),
}

pub type ErExportResult<T> = Result<T, ErExportError>;

pub trait ErDiagramExporter: Send + Sync {
    fn generate_and_export(
        &self,
        tables: &[ErTableInfo],
        filename: &str,
        cache_dir: &Path,
    ) -> ErExportResult<PathBuf>;
}
