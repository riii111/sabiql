use std::error::Error;
use std::path::{Path, PathBuf};

use crate::domain::ErTableInfo;

pub type ErExportResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub trait ErDiagramExporter: Send + Sync {
    fn generate_and_export(
        &self,
        tables: &[ErTableInfo],
        filename: &str,
        cache_dir: &Path,
    ) -> ErExportResult<PathBuf>;
}
