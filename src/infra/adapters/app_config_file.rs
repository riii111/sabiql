use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

pub(super) const CONFIG_FILE_NAME: &str = "connections.toml";

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);
static CONFIG_FILE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn lock() -> MutexGuard<'static, ()> {
    CONFIG_FILE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn get_config_dir() -> Result<PathBuf, std::io::Error> {
    let config_base = dirs::config_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find config directory",
        )
    })?;
    Ok(config_base.join("sabiql"))
}

pub(super) fn config_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CONFIG_FILE_NAME)
}

pub(super) fn write_config_file(config_dir: &Path, content: &str) -> Result<(), std::io::Error> {
    if !config_dir.exists() {
        fs::create_dir_all(config_dir)?;
    }

    let path = config_file_path(config_dir);
    let counter = WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = config_dir.join(format!(
        ".connections.toml.{}-{}.tmp",
        std::process::id(),
        counter,
    ));

    write_config_at(&path, &tmp_path, content)
}

pub(super) fn render_config_file(content: &str) -> String {
    format!(
        "# sabiql configuration\n# WARNING: Connection passwords are stored in plain text\n\n{content}"
    )
}

fn write_config_at(path: &Path, tmp_path: &Path, content: &str) -> Result<(), std::io::Error> {
    let mut file = create_config_temp_file(tmp_path)?;
    let result = file.write_all(content.as_bytes());
    drop(file);

    if let Err(error) = result.and_then(|()| fs::rename(tmp_path, path)) {
        let _ = fs::remove_file(tmp_path);
        return Err(error);
    }
    Ok(())
}

fn create_config_temp_file(path: &Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_old_config_without_leaving_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_file_path(dir.path());
        fs::write(&path, "old config").unwrap();

        write_config_file(dir.path(), "new config").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "new config");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn existing_temporary_file_is_preserved_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_file_path(dir.path());
        let tmp_path = dir.path().join("collision.tmp");
        fs::write(&path, "old config").unwrap();
        fs::write(&tmp_path, "other writer").unwrap();

        let error = write_config_at(&path, &tmp_path, "secret").unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(tmp_path).unwrap(), "other writer");
        assert_eq!(fs::read_to_string(path).unwrap(), "old config");
    }

    #[cfg(unix)]
    #[test]
    fn temporary_symlink_is_rejected_without_changing_target() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let tmp_path = dir.path().join("collision.tmp");
        fs::write(&target, "unrelated").unwrap();
        symlink(&target, &tmp_path).unwrap();

        let error =
            write_config_at(&config_file_path(dir.path()), &tmp_path, "secret").unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(tmp_path.is_symlink());
        assert_eq!(fs::read_to_string(target).unwrap(), "unrelated");
    }

    #[test]
    fn rename_failure_removes_temporary_file_and_preserves_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_file_path(dir.path());
        let tmp_path = dir.path().join("config.tmp");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), "original").unwrap();

        assert!(write_config_at(&path, &tmp_path, "secret").is_err());

        assert!(!tmp_path.exists());
        assert_eq!(fs::read_to_string(path.join("keep")).unwrap(), "original");
    }

    #[cfg(unix)]
    #[test]
    fn permissive_umask_keeps_new_and_replaced_config_private() {
        use std::os::unix::fs::PermissionsExt;
        run_isolated(
            "permissive_umask_keeps_new_and_replaced_config_private",
            "umask 000",
        );
        if std::env::var_os("SABIQL_CONFIG_FILE_CHILD").is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let tmp_path = dir.path().join("new.tmp");

        let file = create_config_temp_file(&tmp_path).unwrap();

        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        let path = config_file_path(dir.path());
        fs::write(&path, "old config").unwrap();
        write_config_file(dir.path(), "secret").unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_failure_removes_temporary_file_and_preserves_old_config() {
        run_isolated(
            "write_failure_removes_temporary_file_and_preserves_old_config",
            "ulimit -f 0; trap '' XFSZ",
        );
        if std::env::var_os("SABIQL_CONFIG_FILE_CHILD").is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = config_file_path(dir.path());
        File::create(&path).unwrap();

        assert!(write_config_file(dir.path(), "secret").is_err());

        assert_eq!(fs::read(path).unwrap(), b"");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    fn run_isolated(test: &str, setup: &str) {
        use std::process::Command;
        if std::env::var_os("SABIQL_CONFIG_FILE_CHILD").is_some() {
            return;
        }
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{setup}; exec \"$1\" --exact \"$2\""))
            .arg("config-file-test")
            .arg(std::env::current_exe().unwrap())
            .arg(format!("adapters::app_config_file::tests::{test}"))
            .env("SABIQL_CONFIG_FILE_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
