use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;

use crate::domain::ErTableInfo;

pub trait ErDiagramExporter: Send + Sync {
    fn generate_and_export(
        &self,
        tables: &[ErTableInfo],
        filename: &str,
        cache_dir: &Path,
    ) -> Result<PathBuf>;
}
