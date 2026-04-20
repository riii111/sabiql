use std::path::PathBuf;

pub trait ErLogWriter: Send + Sync {
    fn write_er_failure_log(
        &self,
        failed_tables: Vec<(String, String)>,
        cache_dir: PathBuf,
    ) -> std::io::Result<()>;
}
